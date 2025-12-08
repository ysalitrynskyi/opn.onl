interface LogoProps {
    className?: string;
    iconOnly?: boolean;
    showFull?: boolean;
}

export default function Logo({ className = "h-8", iconOnly = false, showFull = true }: LogoProps) {
    return (
        <div className={`group flex items-center gap-3 ${className}`}>
            {/* Modern icon */}
            <div className="relative flex items-center justify-center w-10 h-10 rounded-2xl bg-gradient-to-br from-violet-500 via-primary-500 to-cyan-500 p-[2px] shadow-xl shadow-primary-500/20 transition-transform duration-300 group-hover:scale-105">
                <div className="flex items-center justify-center w-full h-full rounded-[14px] bg-white dark:bg-slate-900">
                    {/* Chain link icon */}
                    <svg viewBox="0 0 24 24" className="w-5 h-5" fill="none">
                        <path 
                            d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" 
                            stroke="url(#logoGradient1)" 
                            strokeWidth="2.5" 
                            strokeLinecap="round" 
                            strokeLinejoin="round"
                        />
                        <path 
                            d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" 
                            stroke="url(#logoGradient2)" 
                            strokeWidth="2.5" 
                            strokeLinecap="round" 
                            strokeLinejoin="round"
                        />
                        <defs>
                            <linearGradient id="logoGradient1" x1="10" y1="4" x2="20" y2="14" gradientUnits="userSpaceOnUse">
                                <stop stopColor="#8B5CF6" />
                                <stop offset="1" stopColor="#0EA5E9" />
                            </linearGradient>
                            <linearGradient id="logoGradient2" x1="4" y1="10" x2="14" y2="20" gradientUnits="userSpaceOnUse">
                                <stop stopColor="#0EA5E9" />
                                <stop offset="1" stopColor="#10B981" />
                            </linearGradient>
                        </defs>
                    </svg>
                </div>
            </div>
            
            {!iconOnly && (
                <div className="flex items-center select-none">
                    {showFull ? (
                        // Full version: OPeN.ONLine - modern typography
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
