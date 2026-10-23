//! The multi-instance contract: two `Core`s constructed in the same process
//! stay isolated — independent event buses, push sinks, termination flags and
//! config snapshots.
//!
//! Each shell resolves one process-wide instance via its `core()` singleton,
//! but nothing inside `Core` itself may assume that. If the runtime ever hosts
//! two apps in one process (or a test spins parallel cores), correctness still
//! has to hold — this test pins it.

use std::sync::{Arc, Mutex, atomic::Ordering};

use kitron_core::{Core, EventName};

fn config_json(name: &str, identifier: &str, version: &str) -> String {
    format!(
        r#"{{"app":{{"name":"{name}","identifier":"{identifier}","version":"{version}"}}}}"#
    )
}

#[test]
fn two_cores_are_isolated_instances() {
    // SAFETY: single-threaded test process and each Core reads the environment
    // exactly once, at construction (`from_env` snapshots it into fields).
    // Edition 2024 freezes `std::env::set_var`/`remove_var` behind `unsafe`.
    unsafe {
        std::env::remove_var("KITRON_DEV");
        std::env::set_var("KITRON_ROOT", "/tmp/kitron-multi-a");
        std::env::set_var(
            "KITRON_CONFIG_JSON",
            config_json("local-a", "dev.kitron.multi.a", "1.0.0"),
        );
    }
    let a = Core::from_env();

    unsafe {
        std::env::set_var("KITRON_DEV", "1");
        std::env::set_var("KITRON_ROOT", "/tmp/kitron-multi-b");
        std::env::set_var(
            "KITRON_CONFIG_JSON",
            config_json("local-b", "dev.kitron.multi.b", "2.0.0"),
        );
    }
    let b = Core::from_env();

    // 1. Config, root and dev-mode snapshots are frozen per instance — a
    //    second `from_env` under different env must not mutate the first.
    assert_eq!(a.config.app.name, "local-a");
    assert_eq!(a.config.app.identifier, "dev.kitron.multi.a");
    assert_eq!(a.config.app.version, "1.0.0");
    assert_eq!(a.root_dir, "/tmp/kitron-multi-a");
    assert!(!a.dev_mode);

    assert_eq!(b.config.app.name, "local-b");
    assert_eq!(b.config.app.identifier, "dev.kitron.multi.b");
    assert_eq!(b.config.app.version, "2.0.0");
    assert_eq!(b.root_dir, "/tmp/kitron-multi-b");
    assert!(b.dev_mode);

    // 2. Event buses are independent: an emit on `a` never reaches `b`.
    assert!(!Arc::ptr_eq(&a.events_handle(), &b.events_handle()));
    a.emit(EventName::AppReady, serde_json::json!({"source": "a"}));
    let drained_a = a.events().drain();
    assert_eq!(drained_a.len(), 1);
    assert_eq!(drained_a[0].name, "app.ready");
    assert_eq!(drained_a[0].payload["source"], "a");
    assert_eq!(
        b.events().drain().len(),
        0,
        "an emit on Core `a` must not queue into Core `b`'s bus"
    );

    // 3. Push sinks are independent: only the core holding a sink takes the
    //    push path, and a sink never double-queues on the bus.
    let pushed_b: Arc<Mutex<Vec<(String, serde_json::Value)>>> =
        Arc::new(Mutex::new(Vec::new()));
    let captured_b = Arc::clone(&pushed_b);
    b.set_event_sink(Some(Arc::new(move |name, payload| {
        captured_b
            .lock()
            .unwrap()
            .push((name.as_str().to_string(), payload));
    })));
    b.emit(EventName::MenuAction, serde_json::json!({"action": "b"}));
    a.emit(EventName::AppQuit, serde_json::json!({}));
    assert_eq!(
        b.events().drain().len(),
        0,
        "the sink on `b` must take delivery — nothing may queue behind it"
    );
    assert_eq!(a.events().drain().len(), 1, "no sink on `a`: queued instead");
    let pushed = pushed_b.lock().unwrap().clone();
    assert_eq!(pushed.len(), 1);
    assert_eq!(pushed[0].0, "menu.action");
    assert_eq!(pushed[0].1["action"], "b");

    // 4. Termination state is independent (spec §11 lifecycle moment).
    a.request_termination();
    assert!(a.termination_requested.load(Ordering::SeqCst));
    assert!(
        !b.termination_requested.load(Ordering::SeqCst),
        "termination requested on `a` must not leak into `b`"
    );
    b.termination_cancelled.store(true, Ordering::SeqCst);
    assert!(!a.termination_cancelled.load(Ordering::SeqCst));
}
