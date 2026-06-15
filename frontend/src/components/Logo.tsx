interface LogoProps {
    className?: string;
    iconOnly?: boolean;
    showFull?: boolean;
}

export default function Logo({ className = "h-8", iconOnly = false, showFull = false }: LogoProps) {
    return (
        <div className={`flex items-center gap-2.5 ${className}`}>
            <img
                src="/logo.png"
                alt="opn.onl logo"
                className="h-7 w-auto"
            />

            {!iconOnly && (
                <span className="font-display text-2xl font-extrabold tracking-tightest text-ink leading-none select-none">
                    {showFull ? (
                        <>OPeN<span className="text-primary-600">.</span>ONLine</>
                    ) : (
                        <>opn<span className="text-primary-600">.</span>onl</>
                    )}
                </span>
            )}
        </div>
    );
}
