interface LogoProps {
    className?: string;
    iconOnly?: boolean;
    showFull?: boolean;
}

export default function Logo({ className = "h-8", iconOnly = false, showFull = true }: LogoProps) {
    return (
        <div className={`flex items-center gap-2.5 font-bold text-2xl tracking-tighter ${className}`}>
            {/* Original icon - interlocking circles */}
            <div className="logo-icon relative flex items-center justify-center w-9 h-9 bg-gradient-to-br from-primary-500 to-primary-700 rounded-xl shadow-lg shadow-primary-500/25 overflow-hidden">
                <div className="logo-shine" />
                <svg viewBox="0 0 24 24" className="w-5 h-5 text-white relative z-10" fill="none" stroke="currentColor" strokeWidth="2.5">
                    <circle cx="9" cy="12" r="4" />
                    <circle cx="15" cy="12" r="4" />
                </svg>
            </div>
            
            {!iconOnly && (
                <div className="flex items-center select-none">
                    {showFull ? (
                        // Full version: OPeN.ONLine
                        <div className="flex items-baseline tracking-tight">
                            <span className="text-2xl font-black bg-gradient-to-r from-violet-600 via-primary-600 to-cyan-600 bg-clip-text text-transparent">
                                OPeN
                            </span>
                            <span className="text-2xl font-light text-slate-300 mx-0.5">.</span>
                            <span className="text-2xl font-black bg-gradient-to-r from-cyan-600 to-emerald-500 bg-clip-text text-transparent">
                                ONLine
                            </span>
                        </div>
                    ) : (
                        // Short version: opn.onl
                        <div className="flex items-baseline tracking-tight">
                            <span className="text-2xl font-black bg-gradient-to-r from-violet-600 to-primary-600 bg-clip-text text-transparent">
                                opn
                            </span>
                            <span className="text-2xl font-light text-slate-300 mx-0.5">.</span>
                            <span className="text-2xl font-black bg-gradient-to-r from-cyan-600 to-emerald-500 bg-clip-text text-transparent">
                                onl
                            </span>
                        </div>
                    )}
                </div>
            )}
        </div>
    );
}
