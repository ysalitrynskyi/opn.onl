import { useRef } from 'react';
import { motion } from 'framer-motion';
import { X } from 'lucide-react';
import { useFocusTrap } from '../../hooks/useFocusTrap';
import LinkPreviewCard from '../LinkPreviewCard';
import Sparkline from '../Sparkline';
import type { LinkData } from './types';

interface PreviewModalProps {
    link: LinkData;
    sparkline?: { data: number[]; labels: string[] };
    onClose: () => void;
}

export default function PreviewModal({ link, sparkline, onClose }: PreviewModalProps) {
    const dialogRef = useRef<HTMLDivElement>(null);
    useFocusTrap(dialogRef);

    return (
        <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="fixed inset-0 bg-ink/40 backdrop-blur-sm flex items-center justify-center z-50 p-4"
            onClick={onClose}
        >
            <motion.div
                ref={dialogRef}
                role="dialog"
                aria-modal="true"
                aria-labelledby="preview-dialog-title"
                initial={{ scale: 0.97, opacity: 0, y: 8 }}
                animate={{ scale: 1, opacity: 1, y: 0 }}
                exit={{ scale: 0.97, opacity: 0, y: 8 }}
                transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
                className="bg-surface rounded-2xl border border-line2 shadow-lift max-w-md w-full p-6"
                onClick={e => e.stopPropagation()}
            >
                <div className="flex items-center justify-between mb-5">
                    <h3 id="preview-dialog-title" className="font-display text-lg font-bold text-ink tracking-tight">Link preview</h3>
                    <button onClick={onClose} aria-label="Close" className="text-faint transition-colors hover:text-ink">
                        <X className="h-5 w-5" />
                    </button>
                </div>
                <div className="mb-4">
                    <p className="font-mono text-xs uppercase tracking-[0.14em] text-faint mb-2">Short URL</p>
                    <code className="rounded-md bg-paper border border-line px-2 py-1 font-mono text-sm text-primary-600">
                        {link.short_url}
                    </code>
                </div>
                <div className="mb-4">
                    <p className="font-mono text-xs uppercase tracking-[0.14em] text-faint mb-2">Destination preview</p>
                    <LinkPreviewCard url={link.original_url} />
                </div>
                {sparkline && (
                    <div className="border-t border-line pt-4">
                        <p className="font-mono text-xs uppercase tracking-[0.14em] text-faint mb-2">Clicks (last 7 days)</p>
                        <div className="rounded-lg border border-line bg-paper p-3">
                            <Sparkline
                                data={sparkline.data}
                                labels={sparkline.labels}
                                width={320}
                                height={60}
                                strokeWidth={2}
                            />
                        </div>
                    </div>
                )}
            </motion.div>
        </motion.div>
    );
}
