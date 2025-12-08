import { useEffect, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Check, X, AlertCircle, Info } from 'lucide-react';

export type ToastType = 'success' | 'error' | 'info' | 'warning';

interface ToastProps {
    message: string;
    type?: ToastType;
    duration?: number;
    onClose: () => void;
}

export function Toast({ message, type = 'success', duration = 3000, onClose }: ToastProps) {
    useEffect(() => {
        const timer = setTimeout(onClose, duration);
        return () => clearTimeout(timer);
    }, [duration, onClose]);

    const icons = {
        success: <Check className="h-5 w-5" />,
        error: <X className="h-5 w-5" />,
        warning: <AlertCircle className="h-5 w-5" />,
        info: <Info className="h-5 w-5" />,
    };

    const styles = {
        success: 'bg-emerald-500 text-white',
        error: 'bg-red-500 text-white',
        warning: 'bg-amber-500 text-white',
        info: 'bg-blue-500 text-white',
    };

    return (
        <motion.div
            initial={{ opacity: 0, y: 50, scale: 0.9 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 20, scale: 0.9 }}
            className={`fixed bottom-4 right-4 z-50 flex items-center gap-3 px-4 py-3 rounded-lg shadow-lg ${styles[type]}`}
        >
            {icons[type]}
            <span className="font-medium">{message}</span>
            <button 
                onClick={onClose} 
                className="ml-2 hover:opacity-80 transition-opacity"
                aria-label="Close notification"
            >
                <X className="h-4 w-4" />
            </button>
        </motion.div>
    );
}

// Toast container for managing multiple toasts
interface ToastItem {
    id: number;
    message: string;
    type: ToastType;
}

let toastId = 0;
let addToastFn: ((message: string, type?: ToastType) => void) | null = null;

export function ToastContainer() {
    const [toasts, setToasts] = useState<ToastItem[]>([]);

    useEffect(() => {
        addToastFn = (message: string, type: ToastType = 'success') => {
            const id = ++toastId;
            setToasts(prev => [...prev, { id, message, type }]);
        };
        return () => { addToastFn = null; };
    }, []);

    const removeToast = (id: number) => {
        setToasts(prev => prev.filter(t => t.id !== id));
    };

    return (
        <AnimatePresence>
            {toasts.map((toast, index) => (
                <motion.div
                    key={toast.id}
                    style={{ bottom: `${(index * 70) + 16}px` }}
                    className="fixed right-4 z-50"
                >
                    <Toast
                        message={toast.message}
                        type={toast.type}
                        onClose={() => removeToast(toast.id)}
                    />
                </motion.div>
            ))}
        </AnimatePresence>
    );
}

// Global toast function
export function toast(message: string, type: ToastType = 'success') {
    if (addToastFn) {
        addToastFn(message, type);
    }
}

export default Toast;

