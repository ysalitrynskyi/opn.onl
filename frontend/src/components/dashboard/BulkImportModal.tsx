import { useRef } from 'react';
import { motion } from 'framer-motion';
import { Upload, X } from 'lucide-react';
import { useFocusTrap } from '../../hooks/useFocusTrap';
import { pluralize } from '../../utils/plural';
import { parseBulkUrls, type BulkFailure } from './bulkImport';

export interface BulkReport {
    message: string;
    failures: BulkFailure[];
}

interface BulkImportModalProps {
    urls: string;
    onUrlsChange: (urls: string) => void;
    importing: boolean;
    report: BulkReport | null;
    onImport: () => void;
    onClose: () => void;
}

export default function BulkImportModal({
    urls,
    onUrlsChange,
    importing,
    report,
    onImport,
    onClose,
}: BulkImportModalProps) {
    const dialogRef = useRef<HTMLDivElement>(null);
    useFocusTrap(dialogRef);
    const validCount = parseBulkUrls(urls).length;

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
                aria-labelledby="bulk-import-title"
                initial={{ scale: 0.97, opacity: 0, y: 8 }}
                animate={{ scale: 1, opacity: 1, y: 0 }}
                exit={{ scale: 0.97, opacity: 0, y: 8 }}
                transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
                className="bg-surface rounded-2xl border border-line2 shadow-lift max-w-lg w-full p-6"
                onClick={e => e.stopPropagation()}
            >
                <div className="flex items-center justify-between mb-4">
                    <h3 id="bulk-import-title" className="font-display text-xl font-bold text-ink tracking-tight">Bulk import URLs</h3>
                    <button onClick={onClose} aria-label="Close" className="text-faint transition-colors hover:text-ink">
                        <X className="h-5 w-5" />
                    </button>
                </div>
                <p className="text-sm text-muted mb-4">
                    Paste one URL per line. Each URL will be shortened automatically.
                </p>
                <textarea
                    value={urls}
                    onChange={(e) => onUrlsChange(e.target.value)}
                    readOnly={importing}
                    className="w-full h-48 rounded-lg border border-line2 bg-surface px-4 py-3 font-mono text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint"
                    placeholder="https://example.com/page1&#10;https://example.com/page2&#10;https://example.com/page3"
                />
                {report && (
                    <div role="alert" className="mt-4 rounded-lg border border-danger/30 bg-danger/5 px-4 py-3 text-sm text-danger">
                        <p>{report.message}</p>
                        {report.failures.length > 0 && (
                            <>
                                <ul
                                    tabIndex={0}
                                    aria-label="Failed URLs"
                                    className="mt-2 max-h-40 space-y-1.5 overflow-y-auto"
                                >
                                    {report.failures.map((failure, index) => (
                                        <li key={index} className="break-all">
                                            {failure.url && <span className="font-mono">{failure.url}</span>}
                                            {failure.url && ' '}
                                            <span className="text-danger/80">{failure.reason}</span>
                                        </li>
                                    ))}
                                </ul>
                                {report.failures.some((failure) => failure.url) && (
                                    <p className="mt-2 text-danger/80">
                                        The URLs that failed are left in the box so you can fix them and import again.
                                    </p>
                                )}
                            </>
                        )}
                    </div>
                )}
                <div className="flex justify-between items-center mt-4">
                    <span className="text-sm text-faint">
                        {validCount} valid {pluralize(validCount, 'URL')}
                    </span>
                    <div className="flex gap-3">
                        <button
                            onClick={onClose}
                            className="rounded-lg border border-line2 px-4 py-2 font-medium text-muted transition-colors hover:text-ink hover:border-ink/30"
                        >
                            Cancel
                        </button>
                        <button
                            onClick={onImport}
                            disabled={importing}
                            className="inline-flex items-center gap-2 rounded-lg bg-primary-600 px-4 py-2 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-70"
                        >
                            {importing ? (
                                <div className="h-4 w-4 rounded-full border-2 border-white/30 border-t-white animate-spin" />
                            ) : (
                                <Upload className="h-4 w-4" />
                            )}
                            Import All
                        </button>
                    </div>
                </div>
            </motion.div>
        </motion.div>
    );
}
