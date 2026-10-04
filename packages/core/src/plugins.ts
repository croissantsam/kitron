import type {
  KitronPluginDefinition,
  PluginAPI,
  PluginCommandContext,
} from "@kitron/types";
import { definePlugin } from "@kitron/types";
import { getPeer } from "./transport.js";

export { definePlugin };
export type { KitronPluginDefinition, PluginAPI };

/** Minimal plugin host for the Node backend — §45. */
export class PluginManager {
  private commands = new Map<string, (payload: unknown, ctx?: PluginCommandContext) => unknown>();

  async register(plugin: KitronPluginDefinition): Promise<void> {
    const api: PluginAPI = {
      command: <TPayload, TResult>(
        name: string,
        handler: (payload: TPayload, ctx?: PluginCommandContext) => Promise<TResult> | TResult,
      ) => {
        if (this.commands.has(name)) {
          throw new Error(`Plugin command "${name}" already registered`);
        }
        this.commands.set(
          name,
          handler as (payload: unknown, ctx?: PluginCommandContext) => unknown,
        );
        getPeer().handle(`plugin.${plugin.name}.${name}`, (payload: unknown) =>
          (handler as (p: unknown) => unknown)(payload),
        );
      },
      onEvent: (channel, listener) => getPeer().subscribe(channel, listener),
    };
    await plugin.setup(api);
  }

  commandsOf(): string[] {
    return [...this.commands.keys()];
  }
}
