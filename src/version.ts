import packageJson from "../package.json";

/**
 * Global single source of truth for the application version.
 * Automatically synchronizes from package.json across all UI components.
 */
export const APP_VERSION: string = packageJson.version;
