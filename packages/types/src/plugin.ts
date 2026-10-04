/** Plugin system — §45. */
export interface PluginCommandContext {
  windowId?: string;
}

export interface PluginAPI {
  command<TPayload = unknown, TResult = unknown>(
    name: string,
    handler: (payload: TPayload, ctx?: PluginCommandContext) => Promise<TResult> | TResult,
  ): void;
  onEvent(channel: string, listener: (payload: unknown) => void): () => void;
}

export interface KitronPluginDefinition {
  name: string;
  version?: string;
  setup(api: PluginAPI): void | Promise<void>;
}

export function definePlugin(plugin: KitronPluginDefinition): KitronPluginDefinition {
  if (!plugin.name) throw new Error("Plugin must have a name");
  if (typeof plugin.setup !== "function") {
    throw new Error(`Plugin "${plugin.name}" must define setup()`);
  }
  return plugin;
}
