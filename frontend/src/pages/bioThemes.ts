/** Themes the public bio page renders. An empty id is the default light page.
 *  Keep this in step with `BIO_THEMES` in `backend/src/handlers/bio.rs`.
 */
export const BIO_THEMES = [
    { id: '', name: 'Light' },
    { id: 'ink', name: 'Ink' },
    { id: 'cobalt', name: 'Cobalt' },
] as const;

export type BioThemeId = (typeof BIO_THEMES)[number]['id'];

export function bioThemeId(value: string | null | undefined): BioThemeId {
    return value === 'ink' || value === 'cobalt' ? value : '';
}
