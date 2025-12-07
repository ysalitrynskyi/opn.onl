interface LogoProps {
    className?: string;
    iconOnly?: boolean;
}

export default function Logo({ className = "h-8", iconOnly = false }: LogoProps) {
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
                <span className="bg-clip-text text-transparent bg-gradient-to-r from-slate-900 to-slate-600">
                    opn.onl
                </span>
            )}
        </div>
    );
}
