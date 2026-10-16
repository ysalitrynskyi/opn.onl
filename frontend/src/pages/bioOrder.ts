/** Same cap as `PUT /auth/bio/order` (`MAX_IDS_PER_REQUEST`). */
export const MAX_BIO_ORDER = 500;

/** Whether one order request can carry this many bio links. */
export function bioOrderFits(count: number): boolean {
    return count <= MAX_BIO_ORDER;
}
