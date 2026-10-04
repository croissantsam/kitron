const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("bench", {
  invoke: (channel, payload) => ipcRenderer.invoke(channel, payload),
});
