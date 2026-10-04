async function main(): Promise<void> {
  const pong = await window.kitron?.invoke("ping").catch(() => "offline");

  document.querySelector("#root")!.innerHTML = `
    <main style="font-family: system-ui; padding: 32px">
      <h1>Hello Kitron</h1>
      <p>Vanilla + Vite — ping → ${pong}</p>
      <button id="notify">Notify</button>
      <button id="pin">Always on top</button>
    </main>
  `;

  document.querySelector("#notify")!.addEventListener("click", () => {
    window.kitron?.native.notification.show({ title: "Kitron", body: "Hello" });
  });

  document.querySelector("#pin")!.addEventListener("click", () => {
    window.kitron?.native.window.setAlwaysOnTop(true);
  });
}

void main();
