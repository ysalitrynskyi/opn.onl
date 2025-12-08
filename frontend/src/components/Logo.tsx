interface LogoProps {
    className?: string;
    iconOnly?: boolean;
    showFull?: boolean;
}

export default function Logo({ className = "h-8", iconOnly = false, showFull = false }: LogoProps) {
    return (
        <div className={`flex items-center gap-2.5 font-bold text-2xl tracking-tighter ${className}`}>
            <div className="logo-icon relative flex items-center justify-center w-9 h-9 bg-gradient-to-br from-primary-500 to-primary-700 rounded-xl shadow-lg shadow-primary-500/25 overflow-hidden">
                {/* Shine effect on hover */}
                <div className="logo-shine" />
                
                {/* Interlocking circles - representing links */}
                <svg viewBox="0 0 24 24" className="w-5 h-5 text-white relative z-10" fill="none" stroke="currentColor" strokeWidth="2.5">
                    <circle cx="9" cy="12" r="4" />
                    <circle cx="15" cy="12" r="4" />
                </svg>
            </div>
            
            {!iconOnly && (
                <div className="flex items-baseline">
                    {showFull ? (
                        // Full version: OPeN.ONLine
                        <span className="select-none">
                            <span className="text-primary-600 font-black">OP</span>
                            <span className="text-slate-400 font-normal text-xl">e</span>
                            <span className="text-primary-600 font-black">N</span>
                            <span className="text-slate-300 font-normal">.</span>
                            <span className="text-emerald-600 font-black">ONL</span>
                            <span className="text-slate-400 font-normal text-xl">ine</span>
                        </span>
                    ) : (
                        // Short version: opn.onl with colored emphasis
                        <span className="select-none">
                            <span className="text-primary-600 font-black">opn</span>
                            <span className="text-slate-300">.</span>
                            <span className="text-emerald-600 font-black">onl</span>
                        </span>
                    )}
                </div>
            )}
        </div>
    );
}
