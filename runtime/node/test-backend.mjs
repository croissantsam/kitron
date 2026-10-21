// Smoke-test backend: registers handlers through the KitronNode global.
KitronNode.onInvoke("math.add", (payload) => ({ sum: payload.a + payload.b }));
KitronNode.onInvoke("users.get", async (payload) => {
  await new Promise((resolve) => setTimeout(resolve, 10));
  if (payload?.id === 404) {
    const error = new Error("user not found");
    error.code = "USER_NOT_FOUND";
    throw error;
  }
  return { id: payload.id, name: "Ada" };
});
KitronNode.onInvoke("echo.throw", () => {
  throw new Error("handler exploded");
});
console.info("smoke backend loaded");
