/**
 * One CSV of the breakdowns the link analytics page shows.
 *
 * Columns are `section,key,clicks`. The day section uses the UTC date
 * (`YYYY-MM-DD`) as the key, in chart order, including quiet days. The
 * other sections are the same top rows the page lists. String cells follow
 * the link export (`csv_field`): always quoted, quotes doubled, and a
 * leading apostrophe when the value starts with `=`, `+`, `-`, `@`, TAB
 * or CR. The file starts with a UTF-8 BOM.
 *
 * A truncated response is a sample. The file then starts with a `#` note
 * (no commas, so it stays one cell) giving the sample size and the period
 * total, the same fact as the notice on the page.
 */

/** How many rows of one breakdown the analytics page lists, and exports. */
export const BREAKDOWN_ROW_LIMIT = 10;

const CSV_BOM = '\uFEFF';
const FORMULA_PREFIX = ['=', '+', '-', '@', '\t', '\r'];

export interface AnalyticsCsvStats {
    truncated?: boolean;
    total_clicks: number;
    clicks_by_day: { date: string; count: number }[];
    clicks_by_country: { country: string; count: number }[];
    clicks_by_city: { city: string; country?: string | null; count: number }[];
    clicks_by_device: { device: string; count: number }[];
    clicks_by_browser: { browser: string; count: number }[];
    clicks_by_os: { os: string; count: number }[];
    clicks_by_referer: { referer: string; count: number }[];
}

/** City line on the page and in the file: "Paris, France", or the city alone. */
export function cityBreakdownLabel(city: string, country?: string | null): string {
    return country ? `${city}, ${country}` : city;
}

/** `<code>-analytics-<days>d-YYYY-MM-DD.csv`, so a 7-day file and a
 *  365-day file saved the same UTC day do not share a name. */
export function analyticsCsvFilename(code: string, days: number, now = new Date()): string {
    return `${code}-analytics-${days}d-${now.toISOString().slice(0, 10)}.csv`;
}

/** A CSV cell. See the file comment for the spreadsheet rule. */
function csvField(value: string): string {
    let escaped = value.replaceAll('"', '""');
    if (FORMULA_PREFIX.some((prefix) => value.startsWith(prefix))) {
        escaped = `'${escaped}`;
    }
    return `"${escaped}"`;
}

/** Counts are non-negative integers and stay unquoted, as in the link export.
 *  Anything else is text, so a stray minus cannot become a formula. */
function csvCount(count: number): string {
    if (Number.isSafeInteger(count) && count >= 0) return String(count);
    return csvField(String(count));
}

function shown(value: string | null | undefined): string {
    return String(value ?? 'Unknown');
}

function topRows(rows: { label: string; count: number }[]): { label: string; count: number }[] {
    return [...rows].sort((a, b) => b.count - a.count).slice(0, BREAKDOWN_ROW_LIMIT);
}

function csvRow(section: string, key: string, count: number): string {
    return `${csvField(section)},${csvField(key)},${csvCount(count)}`;
}

export function buildLinkAnalyticsCsv(stats: AnalyticsCsvStats): string {
    const lines: string[] = [];
    if (stats.truncated) {
        const sampled = stats.clicks_by_day.reduce((sum, day) => sum + day.count, 0);
        lines.push(`# Based on the most recent ${sampled} clicks of ${stats.total_clicks} in this period`);
    }
    lines.push('section,key,clicks');

    for (const day of stats.clicks_by_day) {
        lines.push(csvRow('day', day.date, day.count));
    }

    const sections: [string, { label: string; count: number }[]][] = [
        ['country', stats.clicks_by_country.map((item) => ({ label: shown(item.country), count: item.count }))],
        ['city', stats.clicks_by_city.map((item) => ({
            label: shown(cityBreakdownLabel(item.city, item.country)),
            count: item.count,
        }))],
        ['device', stats.clicks_by_device.map((item) => ({ label: shown(item.device), count: item.count }))],
        ['browser', stats.clicks_by_browser.map((item) => ({ label: shown(item.browser), count: item.count }))],
        ['os', stats.clicks_by_os.map((item) => ({ label: shown(item.os), count: item.count }))],
        ['referrer', stats.clicks_by_referer.map((item) => ({ label: shown(item.referer), count: item.count }))],
    ];
    for (const [section, items] of sections) {
        for (const item of topRows(items)) {
            lines.push(csvRow(section, item.label, item.count));
        }
    }

    return `${CSV_BOM}${lines.join('\n')}\n`;
}
