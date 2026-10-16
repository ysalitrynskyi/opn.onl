import { pluralize } from '../../utils/plural';
import type { LinkData } from './types';

/**
 * The sentences `links::inactive_reason()` returns, in the same order as
 * `inactive_kind`. The list payload has no blocklist verdict and no
 * owner-disabled flag, so "blocked" and "disabled" are not guessed here.
 */
export const INACTIVE_REASON = {
    usedUpOnce: 'This one-time link has already been opened',
    notStarted: 'Link is scheduled to activate later',
    expired: 'Link has expired',
    usedUp: 'Link has reached maximum clicks',
} as const;

export interface InactiveBadge {
    /** Short label drawn on the row. */
    label: 'Used up' | 'Not started' | 'Expired';
    /** The exact `inactive_reason()` sentence, for the tooltip. */
    reason: string;
}

type StatusLink = Pick<
    LinkData,
    'click_count' | 'max_clicks' | 'starts_at' | 'expires_at' | 'burn_after_reading' | 'burned_at'
>;

/** True when the click count has met the cap. A missing cap is not a cap. */
export function clickCapReached(link: Pick<LinkData, 'click_count' | 'max_clicks'>): boolean {
    return link.max_clicks != null && link.click_count >= link.max_clicks;
}

/**
 * Why the link is not live, or null while it is. Same precedence as
 * `inactive_kind`: a consumed one-time link, then a start still ahead, then
 * an expiry already past, then a click cap that has been reached.
 */
export function inactiveBadge(link: StatusLink, now = Date.now()): InactiveBadge | null {
    const capped = clickCapReached(link);
    if (link.burned_at || (link.burn_after_reading && capped)) {
        return { label: 'Used up', reason: INACTIVE_REASON.usedUpOnce };
    }

    if (link.starts_at) {
        const starts = Date.parse(link.starts_at);
        if (!Number.isNaN(starts) && now < starts) {
            return { label: 'Not started', reason: INACTIVE_REASON.notStarted };
        }
    }

    if (link.expires_at) {
        const expires = Date.parse(link.expires_at);
        if (!Number.isNaN(expires) && now > expires) {
            return { label: 'Expired', reason: INACTIVE_REASON.expired };
        }
    }

    if (capped) {
        return { label: 'Used up', reason: INACTIVE_REASON.usedUp };
    }

    return null;
}

export interface RowStatus {
    /** Why the link is not live, or null while it is. */
    badge: InactiveBadge | null;
    /** `starts_at` is still ahead. */
    startsLater: boolean;
    /** The list said the link is inactive and nothing on the row says why. */
    inactiveUnexplained: boolean;
}

/**
 * The row's status against the clock as it is now. Rows outlive list reloads
 * (they are keyed by id), so a clock read once per row would keep a link "Not
 * started" after its start and never mark it expired.
 */
export function rowStatus(link: StatusLink & Pick<LinkData, 'is_active'>, now = Date.now()): RowStatus {
    const badge = inactiveBadge(link, now);
    const starts = link.starts_at ? Date.parse(link.starts_at) : NaN;
    const started = !Number.isNaN(starts) && starts <= now;
    return {
        badge,
        startsLater: !Number.isNaN(starts) && !started,
        // `is_active` was decided when the list was fetched. A start that has
        // passed since then is why it was false, and the link is live now.
        inactiveUnexplained: !badge && !link.is_active && !started,
    };
}

/** "Oct 12". English, like the created date under the row, on the viewer's calendar day. */
export function formatBadgeDay(iso: string): string {
    return new Date(iso).toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}

/** "3 / 10 clicks". The noun follows the cap, which is what the ratio is counting toward. */
export function clickLimitLabel(clickCount: number, maxClicks: number): string {
    return `${clickCount.toLocaleString()} / ${maxClicks.toLocaleString()} ${pluralize(maxClicks, 'click')}`;
}
