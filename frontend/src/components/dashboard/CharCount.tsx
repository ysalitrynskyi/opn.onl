import { pluralize } from '../../utils/plural';
import { NEAR_LIMIT_CHARS } from './linkFields';

/** Shown only as the text nears its cap, so an empty note does not carry a counter. */
export default function CharCount({ length, max }: { length: number; max: number }) {
    const remaining = max - length;
    if (remaining > NEAR_LIMIT_CHARS) return null;
    const over = remaining < 0;
    const count = Math.abs(remaining);
    const noun = pluralize(count, 'character');
    return (
        <p className={`mt-1.5 text-xs ${remaining <= 100 ? 'text-danger' : 'text-faint'}`} aria-live="polite">
            {over ? `${count.toLocaleString()} ${noun} over the limit` : `${count.toLocaleString()} ${noun} left`}
        </p>
    );
}
