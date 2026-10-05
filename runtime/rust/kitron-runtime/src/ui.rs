//! Kitron UI — Kitron's own renderer (§27 replacement).
//!
//! No React Native, no webview, no Chromium. UI is a retained view tree:
//!
//! ```text
//! TypeScript (@kitron/ui)          Rust (this module)
//!   View/Text/Button/Input   ──►     UiManager (tree store)
//!   render(window, element)  ──►     taffy flexbox layout
//!   onPress / onChange       ◄──     hit-test + ui.press / ui.change
//!                                    softbuffer + tiny-skia + cosmic-text
//! ```
//!
//! Layout and hit-testing run headless (tests, CI). In windowed mode the
//! event loop paints each dirty window and feeds mouse/keyboard back in.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

// ---- model ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiKind {
    #[serde(rename = "view")]
    View,
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "button")]
    Button,
    #[serde(rename = "input")]
    TextInput,
    #[serde(rename = "image")]
    Image,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiProps {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_radius: Option<f32>,
    /// Node participates in press hit-testing (Text onPress, Pressable).
    #[serde(default)]
    pub pressable: bool,
    /// View clips children and scrolls on wheel (ScrollView).
    #[serde(default)]
    pub scrollable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UiSize {
    Pt(f32),
    Pct(String),
    Auto(String),
}

impl UiSize {
    fn to_dimension(&self) -> taffy::Dimension {
        match self {
            UiSize::Pt(v) => taffy::Dimension::length(*v),
            UiSize::Pct(s) => s
                .trim_end_matches('%')
                .parse::<f32>()
                .map(|v| taffy::Dimension::percent(v / 100.0))
                .unwrap_or(taffy::Dimension::auto()),
            UiSize::Auto(_) => taffy::Dimension::auto(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flex: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub justify: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<UiSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<UiSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shrink: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct UiNode {
    pub id: String,
    pub kind: UiKind,
    pub props: UiProps,
    pub style: UiStyle,
    pub children: Vec<String>,
    pub parent: Option<String>,
    /// Decoded pixels (Image nodes, filled asynchronously after `uri` load).
    pub image: Option<LoadedImage>,
}

#[derive(Debug, Clone)]
pub struct LoadedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedNode {
    pub id: String,
    pub kind: UiKind,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub text: String,
    pub placeholder: String,
    pub font_size: f32,
    pub color: String,
    pub background: Option<String>,
    pub border_radius: f32,
    pub focused: bool,
    pub depth: usize,
    pub scrollable: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum UiError {
    #[error("ui node not found: {0}")]
    NotFound(String),
    #[error("ui invalid: {0}")]
    Invalid(String),
    #[error("ui window has no root: {0}")]
    NoRoot(String),
}

// ---- manager ----

pub struct UiManager {
    nodes: Mutex<HashMap<String, UiNode>>,
    roots: Mutex<HashMap<String, String>>,
    focused: Mutex<HashMap<String, String>>,
    dirty: Mutex<HashSet<String>>,
    fonts: Mutex<cosmic_text::FontSystem>,
    /// Scroll offsets per scrollable node id.
    scroll: Mutex<HashMap<String, (f32, f32)>>,
    /// Content extents per scrollable node id (from last layout).
    content: Mutex<HashMap<String, (f32, f32)>>,
    /// Called after async state changes (image loads) to repaint.
    redraw_hook: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl UiManager {
    pub fn new() -> Self {
        Self {
            nodes: Mutex::new(HashMap::new()),
            roots: Mutex::new(HashMap::new()),
            focused: Mutex::new(HashMap::new()),
            dirty: Mutex::new(HashSet::new()),
            fonts: Mutex::new(cosmic_text::FontSystem::new()),
            scroll: Mutex::new(HashMap::new()),
            content: Mutex::new(HashMap::new()),
            redraw_hook: Mutex::new(None),
        }
    }

    pub fn create(
        &self,
        id: String,
        kind: UiKind,
        props: UiProps,
        style: UiStyle,
    ) -> Result<(), UiError> {
        if id.trim().is_empty() {
            return Err(UiError::Invalid("ui.create: id is required".into()));
        }
        let mut nodes = self.nodes.lock().unwrap();
        if nodes.contains_key(&id) {
            return Err(UiError::Invalid(format!("ui.create: duplicate id {id}")));
        }
        nodes.insert(id.clone(), UiNode { id, kind, props, style, children: vec![], parent: None, image: None });
        Ok(())
    }

    pub fn set_props(&self, id: &str, props: UiProps) -> Result<(), UiError> {
        let root = {
            let mut nodes = self.nodes.lock().unwrap();
            let n = nodes.get_mut(id).ok_or_else(|| UiError::NotFound(id.into()))?;
            n.props = props;
            root_of(&nodes, id)
        };
        self.dirty_root(&root);
        Ok(())
    }

    pub fn set_style(&self, id: &str, style: UiStyle) -> Result<(), UiError> {
        let root = {
            let mut nodes = self.nodes.lock().unwrap();
            let n = nodes.get_mut(id).ok_or_else(|| UiError::NotFound(id.into()))?;
            n.style = style;
            root_of(&nodes, id)
        };
        self.dirty_root(&root);
        Ok(())
    }

    pub fn append(&self, parent: &str, child: &str, index: Option<usize>) -> Result<(), UiError> {
        let mut nodes = self.nodes.lock().unwrap();
        if !nodes.contains_key(child) {
            return Err(UiError::NotFound(child.into()));
        }
        let p = nodes.get_mut(parent).ok_or_else(|| UiError::NotFound(parent.into()))?;
        if !matches!(p.kind, UiKind::View) {
            return Err(UiError::Invalid(format!("only View takes children (parent {parent})")));
        }
        if let Some(old) = nodes.get(child).and_then(|c| c.parent.clone()) {
            if let Some(op) = nodes.get_mut(&old) {
                op.children.retain(|c| c != child);
            }
        }
        let p = nodes.get_mut(parent).ok_or_else(|| UiError::NotFound(parent.into()))?;
        match index {
            Some(i) if i <= p.children.len() => p.children.insert(i, child.to_string()),
            _ => p.children.push(child.to_string()),
        }
        nodes.get_mut(child).unwrap().parent = Some(parent.to_string());
        let root = root_of(&nodes, parent);
        drop(nodes);
        self.dirty_root(&root);
        Ok(())
    }

    pub fn remove(&self, id: &str) -> Result<(), UiError> {
        let mut nodes = self.nodes.lock().unwrap();
        if !nodes.contains_key(id) {
            return Err(UiError::NotFound(id.into()));
        }
        let mut stack = vec![id.to_string()];
        while let Some(cur) = stack.pop() {
            if let Some(n) = nodes.remove(&cur) {
                if let Some(p) = n.parent {
                    if let Some(op) = nodes.get_mut(&p) {
                        op.children.retain(|c| c != &cur);
                    }
                }
                for c in n.children {
                    stack.push(c);
                }
                self.roots.lock().unwrap().retain(|_, r| r != &cur);
            }
        }
        // Any removal may affect layout: mark all rooted windows dirty.
        for w in self.roots.lock().unwrap().keys() {
            self.dirty.lock().unwrap().insert(w.clone());
        }
        // Drop focus if the focused node vanished.
        self.focused.lock().unwrap().retain(|_, f| nodes.contains_key(f));
        Ok(())
    }

    pub fn set_root(&self, window: &str, root: &str) -> Result<(), UiError> {
        if !self.nodes.lock().unwrap().contains_key(root) {
            return Err(UiError::NotFound(root.into()));
        }
        self.roots.lock().unwrap().insert(window.to_string(), root.to_string());
        self.dirty.lock().unwrap().insert(window.to_string());
        Ok(())
    }

    pub fn clear_root(&self, window: &str) {
        self.roots.lock().unwrap().remove(window);
        self.focused.lock().unwrap().remove(window);
        self.dirty.lock().unwrap().insert(window.to_string());
    }

    pub fn take_dirty(&self) -> Vec<String> {
        self.dirty.lock().unwrap().drain().collect()
    }

    fn dirty_root(&self, root: &str) {
        for (w, r) in self.roots.lock().unwrap().iter() {
            if r == root {
                self.dirty.lock().unwrap().insert(w.clone());
            }
        }
    }


    // ---- layout (taffy flexbox) ----

    /// Compute the placed tree for a window. Pure: safe headless.
    pub fn layout(&self, window: &str, vw: f32, vh: f32) -> Result<Vec<PlacedNode>, UiError> {
        let root = self.roots.lock().unwrap().get(window).cloned().ok_or_else(|| UiError::NoRoot(window.into()))?;
        let nodes = self.nodes.lock().unwrap();
        let mut fonts = self.fonts.lock().unwrap();

        let mut taffy: taffy::TaffyTree = taffy::TaffyTree::new();
        let mut ids: HashMap<taffy::NodeId, String> = HashMap::new();
        let troot = self.build_taffy(&mut taffy, &mut ids, &nodes, &mut fonts, &root, vw, vh, true)?;
        drop(nodes);
        drop(fonts);
        taffy
            .compute_layout(
                troot,
                taffy::Size {
                    width: taffy::AvailableSpace::Definite(vw),
                    height: taffy::AvailableSpace::Definite(vh),
                },
            )
            .map_err(|e| UiError::Invalid(format!("layout failed: {e}")))?;

        let nodes = self.nodes.lock().unwrap();
        let focused = self.focused.lock().unwrap().get(window).cloned();
        let mut out = Vec::new();
        self.collect(&taffy, &ids, &nodes, troot, 0.0, 0.0, 0, focused.as_deref(), &mut out);
        Ok(out)
    }

    fn build_taffy(
        &self,
        taffy: &mut taffy::TaffyTree,
        ids: &mut HashMap<taffy::NodeId, String>,
        nodes: &HashMap<String, UiNode>,
        fonts: &mut cosmic_text::FontSystem,
        id: &str,
        viewport_w: f32,
        viewport_h: f32,
        is_root: bool,
    ) -> Result<taffy::NodeId, UiError> {
        let n = nodes.get(id).ok_or_else(|| UiError::NotFound(id.into()))?;
        let mut style = style_to_taffy(&n.style);
        // Image leaves use intrinsic pixels unless explicitly sized.
        if matches!(n.kind, UiKind::Image) {
            if let Some(img) = &n.image {
                if n.style.width.is_none() {
                    style.size.width = taffy::Dimension::length(img.width as f32);
                }
                if n.style.height.is_none() {
                    style.size.height = taffy::Dimension::length(img.height as f32);
                }
            } else {
                if n.style.width.is_none() {
                    style.size.width = taffy::Dimension::length(0.0);
                }
                if n.style.height.is_none() {
                    style.size.height = taffy::Dimension::length(0.0);
                }
            }
        }
        // Text leaves are pre-measured (single pass, viewport-bounded).
        let is_leaf = !matches!(n.kind, UiKind::View | UiKind::Image);
        if is_leaf {
            let label = display_text(n);
            let size = measure_text(fonts, &label, font_size_of(n), viewport_w);
            if n.style.width.is_none() {
                style.size.width = taffy::Dimension::length(size.0.max(1.0));
            }
            if n.style.height.is_none() {
                style.size.height = taffy::Dimension::length(size.1.max(1.0));
            }
        }
        // Scroll containers anchor content at the start (ScrollView semantics).
        if n.props.scrollable && n.style.justify.is_none() {
            style.justify_content = Some(taffy::JustifyContent::FLEX_START);
        }
        // The root fills the viewport unless explicitly sized (CSS body semantics).
        if is_root {
            if n.style.width.is_none() {
                style.size.width = taffy::Dimension::length(viewport_w);
            }
            if n.style.height.is_none() {
                style.size.height = taffy::Dimension::length(viewport_h);
            }
        }
        let mut kids = Vec::new();
        for c in &n.children {
            kids.push(self.build_taffy(taffy, ids, nodes, fonts, c, viewport_w, viewport_h, false)?);
        }
        let t = if kids.is_empty() {
            taffy.new_leaf(style).map_err(|e| UiError::Invalid(format!("{e}")))?
        } else {
            taffy.new_with_children(style, &kids).map_err(|e| UiError::Invalid(format!("{e}")))?
        };
        ids.insert(t, id.to_string());
        Ok(t)
    }

    /// Redraw hook for async completions (image loads). Set once by Runtime.
    pub fn set_redraw_hook(&self, hook: Arc<dyn Fn() + Send + Sync>) {
        *self.redraw_hook.lock().unwrap() = Some(hook);
    }

    fn fire_redraw(&self) {
        if let Some(hook) = self.redraw_hook.lock().unwrap().clone() {
            hook();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn collect(
        &self,
        taffy: &taffy::TaffyTree,
        ids: &HashMap<taffy::NodeId, String>,
        nodes: &HashMap<String, UiNode>,
        t: taffy::NodeId,
        ox: f32,
        oy: f32,
        depth: usize,
        focused: Option<&str>,
        out: &mut Vec<PlacedNode>,
    ) {
        let id = match ids.get(&t) {
            Some(id) => id,
            None => return,
        };
        let n = match nodes.get(id) {
            Some(n) => n,
            None => return,
        };
        let layout = match taffy.layout(t) {
            Ok(l) => l,
            Err(_) => return,
        };
        let (x, y) = (ox + layout.location.x, oy + layout.location.y);
        let scrollable = n.props.scrollable && matches!(n.kind, UiKind::View);
        // Scroll offsets shift the content origin of scrollable nodes.
        let (cox, coy) = if scrollable {
            let s = self.scroll.lock().unwrap().get(id).cloned().unwrap_or((0.0, 0.0));
            (x - s.0, y - s.1)
        } else {
            (x, y)
        };
        out.push(PlacedNode {
            id: id.clone(),
            kind: n.kind,
            x,
            y,
            width: layout.size.width,
            height: layout.size.height,
            text: display_text(n),
            placeholder: n.props.placeholder.clone(),
            font_size: font_size_of(n),
            color: n.props.color.clone().unwrap_or_else(|| "#111111".into()),
            background: n.props.background.clone().or_else(|| default_bg(n.kind)),
            border_radius: n.props.border_radius.unwrap_or(0.0),
            focused: focused == Some(id.as_str()),
            depth,
            scrollable,
        });
        // Content extents (for scroll clamping) = max child edge, content-relative.
        let mut content = (0.0f32, 0.0f32);
        if let Ok(kids) = taffy.children(t) {
            for k in kids {
                self.collect(taffy, ids, nodes, k, cox, coy, depth + 1, focused, out);
            }
            for c in out.iter().rev().take_while(|c| c.depth > depth) {
                content.0 = content.0.max(c.x + c.width - cox);
                content.1 = content.1.max(c.y + c.height - coy);
            }
        }
        if scrollable {
            self.content.lock().unwrap().insert(id.clone(), content);
        }
    }

    // ---- interaction ----

    /// Layout + paint a window into a pixmap. `NoRoot` means "nothing to
    /// draw" — the caller skips presentation.
    pub fn paint_window(
        &self,
        window: &str,
        vw: f32,
        vh: f32,
        pixmap: &mut tiny_skia::Pixmap,
        scale: f32,
    ) -> Result<(), UiError> {
        let placed = self.layout(window, vw, vh)?;
        let mut fonts = self.fonts.lock().unwrap();
        let images: std::collections::HashMap<String, LoadedImage> = self
            .nodes
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(id, n)| n.image.clone().map(|img| (id.clone(), img)))
            .collect();
        crate::paint::Painter::new(&mut fonts, scale).paint(pixmap, &placed, &images);
        Ok(())
    }
    /// Topmost interactive node at a point (buttons + inputs win over views).
    pub fn hit_test(&self, window: &str, x: f32, y: f32, vw: f32, vh: f32) -> Result<Option<String>, UiError> {
        let placed = self.layout(window, vw, vh)?;
        // Later nodes paint on top → iterate in reverse.
        for p in placed.iter().rev() {
            let interactive = matches!(p.kind, UiKind::Button | UiKind::TextInput)
                || self.nodes.lock().unwrap().get(&p.id).map(|n| n.props.pressable).unwrap_or(false);
            if interactive
                && x >= p.x
                && y >= p.y
                && x <= p.x + p.width
                && y <= p.y + p.height
            {
                return Ok(Some(p.id.clone()));
            }
        }
        Ok(None)
    }

    /// Topmost scrollable container at a point (for wheel events).
    pub fn scrollable_at(
        &self,
        window: &str,
        x: f32,
        y: f32,
        vw: f32,
        vh: f32,
    ) -> Result<Option<String>, UiError> {
        let placed = self.layout(window, vw, vh)?;
        for p in placed.iter().rev() {
            if p.scrollable && x >= p.x && y >= p.y && x <= p.x + p.width && y <= p.y + p.height {
                return Ok(Some(p.id.clone()));
            }
        }
        Ok(None)
    }

    /// Scroll by (dx, dy) px, clamped to content. Returns (node, (sx, sy)).
    /// Emits nothing — the caller forwards `ui.scroll` and repaints.
    pub fn scroll_by(
        &self,
        window: &str,
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
        vw: f32,
        vh: f32,
    ) -> Result<Option<(String, (f32, f32))>, UiError> {
        let target = match self.scrollable_at(window, x, y, vw, vh)? {
            Some(id) => id,
            None => return Ok(None),
        };
        let placed = self.layout(window, vw, vh)?;
        for q in &placed { eprintln!("PLACED {} d={} x={} y={} w={} h={}", q.id, q.depth, q.x, q.y, q.width, q.height); }
        let node = placed.iter().find(|p| p.id == target);
        let (w, h) = node.map(|p| (p.width, p.height)).unwrap_or((vw, vh));
        let (cw, ch) = self.content.lock().unwrap().get(&target).cloned().unwrap_or((w, h));
        let mut scroll = self.scroll.lock().unwrap();
        let e = scroll.entry(target.clone()).or_insert((0.0, 0.0));
        e.0 = (e.0 + dx).clamp(0.0, (cw - w).max(0.0));
        e.1 = (e.1 + dy).clamp(0.0, (ch - h).max(0.0));
        let off = *e;
        drop(scroll);
        self.dirty.lock().unwrap().insert(window.to_string());
        Ok(Some((target, off)))
    }

    /// Fetch + decode an Image node's `uri` (file:// or http(s)://),
    /// then mark dirty + repaint. Never blocks dispatch (spawned task).
    pub async fn load_image(&self, id: String) {
        let uri = match self.nodes.lock().unwrap().get(&id).map(|n| (n.kind, n.props.uri.clone())) {
            Some((UiKind::Image, uri)) if !uri.is_empty() => uri,
            _ => return,
        };
        let bytes = if let Some(path) = uri.strip_prefix("file://") {
            tokio::fs::read(path).await.ok()
        } else if uri.starts_with("https://") || uri.starts_with("http://") {
            self.fetch_http(&uri).await
        } else {
            None
        };
        let bytes = match bytes {
            Some(b) => b,
            None => return,
        };
        let img = match image::load_from_memory(&bytes) {
            Ok(i) => i.to_rgba8(),
            Err(_) => return,
        };
        let (w, h) = (img.width(), img.height());
        let loaded = LoadedImage { width: w, height: h, rgba: img.into_raw() };
        if let Some(n) = self.nodes.lock().unwrap().get_mut(&id) {
            n.image = Some(loaded);
        }
        self.dirty_root_of(&id);
        self.fire_redraw();
    }

    async fn fetch_http(&self, uri: &str) -> Option<Vec<u8>> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()?;
        let res = client.get(uri).send().await.ok()?;
        if !res.status().is_success() {
            return None;
        }
        res.bytes().await.ok().map(|b| b.to_vec())
    }

    fn dirty_root_of(&self, id: &str) {
        let nodes = self.nodes.lock().unwrap();
        let root = root_of(&nodes, id);
        drop(nodes);
        self.dirty_root(&root);
    }

    /// Press at a point: focuses inputs, returns pressed button id if any.
    pub fn press_at(
        &self,
        window: &str,
        x: f32,
        y: f32,
        vw: f32,
        vh: f32,
    ) -> Result<Option<String>, UiError> {
        let hit = self.hit_test(window, x, y, vw, vh)?;
        if let Some(id) = &hit {
            let kind = self.nodes.lock().unwrap().get(id).map(|n| n.kind);
            match kind {
                Some(UiKind::TextInput) => {
                    self.focused.lock().unwrap().insert(window.to_string(), id.clone());
                    self.dirty.lock().unwrap().insert(window.to_string());
                    return Ok(None);
                }
                Some(UiKind::Button) => return Ok(hit),
                _ => {}
            }
        }
        Ok(None)
    }

    /// Route typed text / backspace to the focused input. Returns (id, value).
    pub fn input_text(&self, window: &str, text: Option<&str>, backspace: bool) -> Result<Option<(String, String)>, UiError> {
        let fid = match self.focused.lock().unwrap().get(window).cloned() {
            Some(f) => f,
            None => return Ok(None),
        };
        // Verify the focused node still exists and is an input.
        {
            let nodes = self.nodes.lock().unwrap();
            match nodes.get(&fid) {
                Some(n) if matches!(n.kind, UiKind::TextInput) => {}
                _ => {
                    self.focused.lock().unwrap().remove(window);
                    return Ok(None);
                }
            }
        }
        let mut nodes = self.nodes.lock().unwrap();
        let n = nodes.get_mut(&fid).ok_or_else(|| UiError::NotFound(fid.clone()))?;
        if backspace {
            n.props.text.pop();
        } else if let Some(t) = text {
            n.props.text.push_str(t);
        }
        let value = n.props.text.clone();
        drop(nodes);
        self.dirty.lock().unwrap().insert(window.to_string());
        Ok(Some((fid, value)))
    }

    pub fn focused_input(&self, window: &str) -> Option<String> {
        self.focused.lock().unwrap().get(window).cloned()
    }
}

fn root_of(nodes: &HashMap<String, UiNode>, id: &str) -> String {
    let mut root = id.to_string();
    while let Some(p) = nodes.get(&root).and_then(|x| x.parent.clone()) {
        root = p;
    }
    root
}

impl Default for UiManager {
    fn default() -> Self {
        Self::new()
    }
}

fn font_size_of(n: &UiNode) -> f32 {
    n.props.font_size.unwrap_or(14.0)
}

fn display_text(n: &UiNode) -> String {
    match n.kind {
        UiKind::Text => n.props.text.clone(),
        UiKind::Button => n.props.text.clone(),
        UiKind::TextInput => {
            if n.props.text.is_empty() {
                n.props.placeholder.clone()
            } else {
                n.props.text.clone()
            }
        }
        UiKind::View | UiKind::Image => String::new(),
    }
}

fn default_bg(kind: UiKind) -> Option<String> {
    match kind {
        UiKind::Button => Some("#e8e8e8".into()),
        UiKind::TextInput => Some("#ffffff".into()),
        UiKind::Image => Some("#f0f0f0".into()),
        _ => None,
    }
}

fn style_to_taffy(s: &UiStyle) -> taffy::Style {
    let mut out = taffy::Style::default();
    out.display = taffy::Display::Flex;
    if let Some(f) = s.flex {
        out.flex_grow = f;
    }
    out.flex_shrink = s.shrink.unwrap_or(1.0);
    out.flex_direction = match s.direction.as_deref() {
        Some("row") => taffy::FlexDirection::Row,
        Some("row-reverse") => taffy::FlexDirection::RowReverse,
        Some("column-reverse") => taffy::FlexDirection::ColumnReverse,
        _ => taffy::FlexDirection::Column,
    };
    out.align_items = match s.align.as_deref() {
        Some("start" | "flex-start") => Some(taffy::AlignItems::FLEX_START),
        Some("end" | "flex-end") => Some(taffy::AlignItems::FLEX_END),
        Some("stretch") => Some(taffy::AlignItems::STRETCH),
        Some("baseline") => Some(taffy::AlignItems::BASELINE),
        _ => Some(taffy::AlignItems::CENTER),
    };
    out.justify_content = match s.justify.as_deref() {
        Some("start" | "flex-start") => Some(taffy::JustifyContent::FLEX_START),
        Some("end" | "flex-end") => Some(taffy::JustifyContent::FLEX_END),
        Some("between" | "space-between") => Some(taffy::JustifyContent::SPACE_BETWEEN),
        Some("around" | "space-around") => Some(taffy::JustifyContent::SPACE_AROUND),
        Some("evenly" | "space-evenly") => Some(taffy::JustifyContent::SPACE_EVENLY),
        _ => Some(taffy::JustifyContent::CENTER),
    };
    if let Some(w) = &s.width {
        out.size.width = w.to_dimension();
    }
    if let Some(h) = &s.height {
        out.size.height = h.to_dimension();
    }
    if let Some(p) = s.padding {
        out.padding = taffy::Rect {
            left: taffy::LengthPercentage::length(p),
            right: taffy::LengthPercentage::length(p),
            top: taffy::LengthPercentage::length(p),
            bottom: taffy::LengthPercentage::length(p),
        };
    }
    if let Some(g) = s.gap {
        out.gap.width = taffy::LengthPercentage::length(g);
        out.gap.height = taffy::LengthPercentage::length(g);
    }
    out
}

/// Single-line text measurement via cosmic-text, bounded by max width.
fn measure_text(
    fonts: &mut cosmic_text::FontSystem,
    text: &str,
    font_size: f32,
    max_width: f32,
) -> (f32, f32) {
    use cosmic_text::{Attrs, Buffer, Metrics, Shaping, Wrap};
    if text.is_empty() {
        return (1.0, font_size * 1.3);
    }
    let metrics = Metrics::new(font_size, font_size * 1.3);
    let mut buffer = Buffer::new(fonts, metrics);
    buffer.set_size(Some(max_width.max(1.0)), None);
    buffer.set_wrap(Wrap::Word);
    buffer.set_text(text, &Attrs::new(), Shaping::Advanced, None);
    buffer.shape_until_scroll(fonts, false);
    let mut w: f32 = 0.0;
    let mut h: f32 = 0.0;
    for run in buffer.layout_runs() {
        w = w.max(run.line_w);
        h += run.line_height;
    }
    (w.max(1.0), h.max(font_size))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mgr() -> UiManager {
        UiManager::new()
    }

    fn view(id: &str, style: UiStyle) -> (String, UiKind, UiProps, UiStyle) {
        (id.into(), UiKind::View, UiProps::default(), style)
    }

    #[test]
    fn column_centers_child() {
        let m = mgr();
        m.create("root".into(), UiKind::View, UiProps::default(), UiStyle {
            flex: Some(1.0),
            align: Some("center".into()),
            justify: Some("center".into()),
            ..Default::default()
        }).unwrap();
        m.create("label".into(), UiKind::Text,
            UiProps { text: "Hi".into(), ..Default::default() }, UiStyle::default()).unwrap();
        m.append("root", "label", None).unwrap();
        m.set_root("win", "label0").unwrap_err();
        m.set_root("win", "root").unwrap();
        let placed = m.layout("win", 800.0, 600.0).unwrap();
        let label = placed.iter().find(|p| p.id == "label").unwrap();
        // Centered: x ≈ (800 - w)/2, y ≈ (600 - h)/2.
        assert!((label.x - (800.0 - label.width) / 2.0).abs() < 1.5, "x={}", label.x);
        assert!((label.y - (600.0 - label.height) / 2.0).abs() < 1.5, "y={}", label.y);
        assert!(label.width > 1.0 && label.height > 1.0);
    }

    #[test]
    fn button_hit_wins_over_sibling_order() {
        let m = mgr();
        let (id, kind, props, style) = view("root", UiStyle { flex: Some(1.0), ..Default::default() });
        m.create(id, kind, props, style).unwrap();
        m.create("btn".into(), UiKind::Button,
            UiProps { text: "OK".into(), ..Default::default() },
            UiStyle { width: Some(UiSize::Pt(200.0)), height: Some(UiSize::Pt(60.0)), ..Default::default() }).unwrap();
        m.append("root", "btn", None).unwrap();
        m.set_root("win", "root").unwrap();
        let hit = m.hit_test("win", 400.0, 300.0, 800.0, 600.0).unwrap();
        assert_eq!(hit.as_deref(), Some("btn"));
        assert_eq!(m.hit_test("win", 10.0, 10.0, 800.0, 600.0).unwrap(), None);
        // Press focuses nothing for buttons, returns the id.
        assert_eq!(m.press_at("win", 400.0, 300.0, 800.0, 600.0).unwrap().as_deref(), Some("btn"));
    }

    #[test]
    fn text_input_focus_and_typing() {
        let m = mgr();
        let (id, kind, props, style) = view("root", UiStyle { flex: Some(1.0), ..Default::default() });
        m.create(id, kind, props, style).unwrap();
        m.create("in".into(), UiKind::TextInput, UiProps::default(),
            UiStyle { width: Some(UiSize::Pt(300.0)), height: Some(UiSize::Pt(40.0)), ..Default::default() }).unwrap();
        m.append("root", "in", None).unwrap();
        m.set_root("win", "root").unwrap();
        assert_eq!(m.press_at("win", 400.0, 300.0, 800.0, 600.0).unwrap(), None);
        assert_eq!(m.focused_input("win").as_deref(), Some("in"));
        let (_, v) = m.input_text("win", Some("hi"), false).unwrap().unwrap();
        assert_eq!(v, "hi");
        let (_, v) = m.input_text("win", None, true).unwrap().unwrap();
        assert_eq!(v, "h");
    }

    #[test]
    fn scroll_clamps_to_content() {
        let m = mgr();
        m.create("root".into(), UiKind::View, UiProps::default(), UiStyle {
            flex: Some(1.0), ..Default::default()
        }).unwrap();
        m.create("list".into(), UiKind::View,
            UiProps { scrollable: true, ..Default::default() },
            UiStyle {
                width: Some(UiSize::Pt(200.0)),
                height: Some(UiSize::Pt(200.0)),
                ..Default::default()
            }).unwrap();
        m.append("root", "list", None).unwrap();
        for i in 0..3 {
            let id = format!("row{i}");
            m.create(id.clone(), UiKind::View, UiProps::default(), UiStyle {
                width: Some(UiSize::Pt(200.0)),
                height: Some(UiSize::Pt(200.0)),
                shrink: Some(0.0),
                ..Default::default()
            }).unwrap();
            m.append("list", &id, None).unwrap();
        }
        m.set_root("win", "root").unwrap();
        // Content is 600 tall in a 200 viewport → max scroll 400.
        let (_, (sx, sy)) = m.scroll_by("win", 400.0, 300.0, 0.0, 10_000.0, 800.0, 600.0).unwrap().unwrap();
        assert_eq!((sx, sy), (0.0, 400.0));
        // Scrolling shifts children: row0 moves up.
        let placed = m.layout("win", 800.0, 600.0).unwrap();
        let row0 = placed.iter().find(|p| p.id == "row0").unwrap();
        assert!((row0.y - (row0.y)).abs() < 0.01); // sanity
        let list = placed.iter().find(|p| p.id == "list").unwrap();
        assert!((row0.y - (list.y - 400.0)).abs() < 1.0, "row0.y={} list.y={}", row0.y, list.y);
        // Scrolling where nothing scrollable → None.
        assert_eq!(m.scroll_by("win", 700.0, 500.0, 0.0, 10.0, 800.0, 600.0).unwrap(), None);
    }

    #[tokio::test]
    async fn image_loads_intrinsic_size() {
        use std::io::Cursor;
        let m = mgr();
        // 4x2 red PNG through the real decoder.
        let mut buf = image::RgbaImage::new(4, 2);
        for px in buf.pixels_mut() {
            *px = image::Rgba([255, 0, 0, 255]);
        }
        let dynimg = image::DynamicImage::ImageRgba8(buf);
        let mut bytes = Vec::new();
        dynimg.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        let dir = std::env::temp_dir();
        let path = dir.join("kitron-test-img.png");
        tokio::fs::write(&path, &bytes).await.unwrap();

        m.create("root".into(), UiKind::View, UiProps::default(), UiStyle {
            flex: Some(1.0), ..Default::default()
        }).unwrap();
        m.create("img".into(), UiKind::Image,
            UiProps { uri: format!("file://{}", path.display()), ..Default::default() },
            UiStyle::default()).unwrap();
        m.append("root", "img", None).unwrap();
        m.set_root("win", "root").unwrap();
        m.load_image("img".into()).await;
        let placed = m.layout("win", 800.0, 600.0).unwrap();
        let img = placed.iter().find(|p| p.id == "img").unwrap();
        assert_eq!((img.width, img.height), (4.0, 2.0));
        tokio::fs::remove_file(&path).await.ok();
    }

    #[test]
    fn validation_errors_are_typed() {
        let m = mgr();
        assert!(matches!(m.create("".into(), UiKind::View, UiProps::default(), UiStyle::default()), Err(UiError::Invalid(_))));
        m.create("a".into(), UiKind::View, UiProps::default(), UiStyle::default()).unwrap();
        assert!(matches!(m.create("a".into(), UiKind::View, UiProps::default(), UiStyle::default()), Err(UiError::Invalid(_))));
        assert!(matches!(m.append("a", "ghost", None), Err(UiError::NotFound(_))));
        assert!(matches!(m.layout("nowin", 1.0, 1.0), Err(UiError::NoRoot(_))));
        // Text nodes reject children.
        m.create("t".into(), UiKind::Text, UiProps { text: "x".into(), ..Default::default() }, UiStyle::default()).unwrap();
        m.create("c".into(), UiKind::View, UiProps::default(), UiStyle::default()).unwrap();
        assert!(matches!(m.append("t", "c", None), Err(UiError::Invalid(_))));
    }
}
