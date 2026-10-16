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

const UTC_DAY_MS = 24 * 60 * 60 * 1000;
const UTC_DATE = /^(\d{4})-(\d{2})-(\d{2})$/;

function utcDayAfter(isoDate: string): string {
    const match = UTC_DATE.exec(isoDate);
    if (!match) return isoDate;
    const next = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3]) + 1));
    return next.toISOString().slice(0, 10);
}

/** One row per UTC day from `windowDays` before `now` through today.
 *  Missing days are 0. A day later than today that is already present
 *  stays, and the gap up to it is filled; days before the window are
 *  dropped. `windowDays` is a duration (`now - N days`), the same cut as
 *  `zero_filled_days` in the analytics handler. The overview chart passes
 *  30, which is the dashboard series. */
export function zeroFillUtcDays(
    clicks: DayCount[],
    windowDays: number,
    now: Date = new Date(),
): DayCount[] {
    const counts = new Map<string, number>();
    for (const row of clicks) {
        if (!UTC_DATE.test(row.date)) continue;
        counts.set(row.date, (counts.get(row.date) ?? 0) + row.count);
    }

    const today = now.toISOString().slice(0, 10);
    const first = new Date(now.getTime() - windowDays * UTC_DAY_MS).toISOString().slice(0, 10);
    let last = today;
    for (const date of counts.keys()) {
        if (date > last) last = date;
    }

    const filled: DayCount[] = [];
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
