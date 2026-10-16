import { useState } from 'react';
import { TrendingUp, Calendar } from 'lucide-react';
import type { LinkData } from './types';

// Compact per-link metadata row: average clicks/day and created date.
// Folder and tag chips live on the row, so a link is not labelled twice.
export default function MiniStats({ link }: { link: LinkData }) {
    const createdDate = new Date(link.created_at);
    const [now] = useState(() => Date.now());
    const daysSinceCreation = Math.max(1, Math.floor((now - createdDate.getTime()) / (1000 * 60 * 60 * 24)));
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
        </div>
    );
}
