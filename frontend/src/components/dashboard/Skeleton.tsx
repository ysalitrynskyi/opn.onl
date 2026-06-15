export default function Skeleton() {
    return (
        <div className="animate-pulse divide-y divide-line border-y border-line">
            {[1, 2, 3].map(i => (
                <div key={i} className="py-5">
                    <div className="flex items-center justify-between gap-4">
                        <div className="space-y-3 flex-1">
                            <div className="h-5 w-48 rounded bg-line2/70" />
                            <div className="h-4 w-72 rounded bg-line" />
                        </div>
                        <div className="flex items-center gap-4">
                            <div className="h-8 w-20 rounded bg-line" />
                            <div className="h-8 w-8 rounded bg-line" />
                        </div>
                    </div>
                </div>
            ))}
        </div>
    );
}
