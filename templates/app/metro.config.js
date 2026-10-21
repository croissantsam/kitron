const { getDefaultConfig, mergeConfig } = require("@react-native/metro-config");

/**
 * Metro configuration for a Kitron app.
 *
 * `kitron dev` passes the repo-specific watch folders when the app lives
 * inside a Kitron workspace; standalone apps use the plain defaults.
 */
const extraWatchFolders = process.env.KITRON_METRO_WATCH_FOLDERS
  ? process.env.KITRON_METRO_WATCH_FOLDERS.split(":").filter(Boolean)
  : [];

module.exports = (() => {
  const defaults = getDefaultConfig(__dirname);
  if (extraWatchFolders.length === 0) {
    return defaults;
  }
  return mergeConfig(defaults, { watchFolders: extraWatchFolders });
})();
