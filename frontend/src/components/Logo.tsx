interface LogoProps {
    className?: string;
    iconOnly?: boolean;
    showFull?: boolean;
}

export default function Logo({ className = "h-8", iconOnly = false, showFull = true }: LogoProps) {
    return (
        <div className={`flex items-center gap-2.5 font-bold text-2xl tracking-tighter ${className}`}>
            {/* Use the actual favicon.svg from public folder */}
            <img 
                src="/favicon.svg" 
                alt="opn.onl logo" 
                className="w-10 h-10 rounded-xl shadow-lg shadow-indigo-500/25"
            />
            
            {!iconOnly && (
                <div className="flex items-center select-none">
                    {showFull ? (
                        // Full version: OPeN.ONLine
                        <div className="flex items-baseline tracking-tight">
                            <span className="text-2xl font-black bg-gradient-to-r from-indigo-500 to-indigo-600 bg-clip-text text-transparent">
                                OPeN
                            </span>
                            <span className="text-2xl font-light text-slate-400 mx-0.5">.</span>
                            <span className="text-2xl font-black bg-gradient-to-r from-indigo-600 to-indigo-500 bg-clip-text text-transparent">
                                ONLine
                            </span>
                        </div>
                    ) : (
                        // Short version: opn.onl
                        <div className="flex items-baseline tracking-tight">
                            <span className="text-2xl font-black bg-gradient-to-r from-indigo-500 to-indigo-600 bg-clip-text text-transparent">
                                opn
                            </span>
                            <span className="text-2xl font-light text-slate-400 mx-0.5">.</span>
                            <span className="text-2xl font-black bg-gradient-to-r from-indigo-600 to-indigo-500 bg-clip-text text-transparent">
                                onl
                            </span>
                        </div>
                    )}
                </div>
            )}
        </div>
    );
}
