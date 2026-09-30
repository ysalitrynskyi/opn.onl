/** Local Y-M-D. toISOString() is UTC and can be tomorrow (or yesterday) for the user. */
export function localIsoDate(date = new Date()): string {
    const year = date.getFullYear();
    const month = String(date.getMonth() + 1).padStart(2, '0');
    const day = String(date.getDate()).padStart(2, '0');
    return `${year}-${month}-${day}`;
}
