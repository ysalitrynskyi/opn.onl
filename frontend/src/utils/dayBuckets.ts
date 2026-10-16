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

const UTC_DATE = /^(\d{4})-(\d{2})-(\d{2})$/;

function utcDayAfter(isoDate: string): string {
    const match = UTC_DATE.exec(isoDate);
    if (!match) return isoDate;
    const next = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3]) + 1));
    return next.toISOString().slice(0, 10);
}

/** One row per UTC day from the earliest payload day through the latest.
 *  Missing days in that span are 0. The server already chose the window
 *  (`clicks_by_day`); the browser clock must not drop a day it sent or
 *  invent one it did not. Days that are not `YYYY-MM-DD` are ignored. */
export function zeroFillUtcDays(clicks: DayCount[]): DayCount[] {
    const counts = new Map<string, number>();
    for (const row of clicks) {
        if (!UTC_DATE.test(row.date)) continue;
        counts.set(row.date, (counts.get(row.date) ?? 0) + row.count);
    }
    if (counts.size === 0) return [];

    let first = '';
    let last = '';
    for (const date of counts.keys()) {
        if (first === '' || date < first) first = date;
        if (last === '' || date > last) last = date;
    }

    const filled: DayCount[] = [];
    // 400 stops a bogus pair of dates from looping. The overview series is
    // about 30 days.
    for (let date = first, guard = 0; date <= last && guard < 400; date = utcDayAfter(date), guard += 1) {
        filled.push({ date, count: counts.get(date) ?? 0 });
    }
    return filled;
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
