import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";

function App() {
  const [pong, setPong] = useState("…");
  useEffect(() => {
    window.kitron?.invoke<string>("ping").then(setPong).catch(() => setPong("offline"));
  }, []);
  return (
    <main style={{ fontFamily: "system-ui", padding: 32 }}>
      <h1>Hello Kitron</h1>
      <p>React + Vite — ping → {pong}</p>
      <button
        onClick={() =>
          window.kitron?.invoke("native.notification.show", { title: "Kitron", body: "Hello" })
        }
      >
        Notify
      </button>
    </main>
  );
}

createRoot(document.getElementById("root")!).render(<App />);
