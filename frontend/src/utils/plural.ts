/**
 * The noun to put after a count: singular for exactly one, plural otherwise,
 * so `${n} ${pluralize(n, 'link')}` reads "0 links", "1 link", "2 links".
 * Pass the plural form when it is not simply the singular plus "s".
 */
export function pluralize(count: number, singular: string, plural = `${singular}s`): string {
    return count === 1 ? singular : plural;
}
