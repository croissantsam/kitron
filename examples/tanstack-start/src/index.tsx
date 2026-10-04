import React, { useEffect, useState } from "react";

export default function Home() {
  const [pong, setPong] = useState("…");
  useEffect(() => {
    window.kitron?.invoke<string>("ping").then(setPong).catch(() => setPong("offline"));
  }, []);
  return (
    <main style={{ fontFamily: "system-ui", padding: 32 }}>
      <h1>Hello Kitron</h1>
      <p>TanStack Start route — ping → {pong}</p>
    </main>
  );
}
