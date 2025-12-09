/**
 * Logger utility for consistent error handling
 * In production, errors are silently logged; in development, they're visible in console
 */

const isDev = import.meta.env.DEV;

export const logger = {
    error: (message: string, ...args: unknown[]) => {
        if (isDev) {
            console.error(`[ERROR] ${message}`, ...args);
        }
        // In production, you could send to an error tracking service here
        // e.g., Sentry, LogRocket, etc.
    },
    
    warn: (message: string, ...args: unknown[]) => {
        if (isDev) {
            console.warn(`[WARN] ${message}`, ...args);
        }
    },
    
    info: (message: string, ...args: unknown[]) => {
        if (isDev) {
            console.info(`[INFO] ${message}`, ...args);
        }
    },
    
    debug: (message: string, ...args: unknown[]) => {
        if (isDev) {
            console.debug(`[DEBUG] ${message}`, ...args);
        }
    },
};

export default logger;


