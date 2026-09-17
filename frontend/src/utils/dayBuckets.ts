const MONTH_SHORT = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

export interface DayCount {
    date: string;
    count: number;
}

/** Backend `clicks_by_day.date` is a UTC calendar day (`YYYY-MM-DD`).
 *  `new Date('YYYY-MM-DD')` is UTC midnight, which formats as the previous
 *  local date west of UTC. */
export function formatDayBucketLabel(isoDate: string): string {
    const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(isoDate);
    if (!match) return isoDate;
    const month = Number(match[2]);
    const day = Number(match[3]);
    if (month < 1 || month > 12 || day < 1) return isoDate;
    return `${MONTH_SHORT[month - 1]} ${day}`;
}

/** Inclusive window of UTC calendar days ending at `now`. Compares the
 *  `YYYY-MM-DD` strings directly so time-of-day and local TZ cannot drop
 *  the first day of the window. */
export function sumClicksInUtcWindow(
    clicks: DayCount[],
    days: number,
    now: Date = new Date(),
): number {
    const start = new Date(now.getTime());
    start.setUTCDate(start.getUTCDate() - days);
    const from = start.toISOString().slice(0, 10);
    return clicks.filter(d => d.date >= from).reduce((sum, d) => sum + d.count, 0);
}
