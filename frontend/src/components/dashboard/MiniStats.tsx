import { TrendingUp, Calendar } from 'lucide-react';
import type { LinkData } from './types';

// Compact per-link metadata row: average clicks/day, created date, and tags.
export default function MiniStats({ link }: { link: LinkData }) {
    const createdDate = new Date(link.created_at);
    const daysSinceCreation = Math.max(1, Math.floor((Date.now() - createdDate.getTime()) / (1000 * 60 * 60 * 24)));
    const avgClicksPerDay = (link.click_count / daysSinceCreation).toFixed(1);

    return (
        <div className="flex items-center gap-4 mt-2 text-xs text-faint">
            <span className="inline-flex items-center gap-1">
                <TrendingUp className="h-3 w-3" />
                {avgClicksPerDay}/day avg
            </span>
            <span className="inline-flex items-center gap-1">
                <Calendar className="h-3 w-3" />
                {createdDate.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })}
            </span>
            {link.tags && link.tags.length > 0 && (
                <div className="flex items-center gap-1.5">
                    {link.tags.slice(0, 2).map(tag => (
                        <span
                            key={tag.id}
                            className="inline-flex items-center gap-1 rounded border border-line px-1.5 py-0.5 text-muted"
                        >
                            <span className="h-2 w-2 rounded-full" style={{ backgroundColor: tag.color }} aria-hidden="true" />
                            {tag.name}
                        </span>
                    ))}
                    {link.tags.length > 2 && (
                        <span className="text-faint">+{link.tags.length - 2}</span>
                    )}
                </div>
            )}
        </div>
    );
}
