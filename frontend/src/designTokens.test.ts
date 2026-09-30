import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

// WCAG 2.x contrast for the colour tokens in tailwind.config.js. The tokens
// are oklch(); convert them to 8-bit sRGB the way a browser paints them, then
// apply the WCAG relative-luminance formula.

type Rgb = [number, number, number]; // 0-255

function oklchToRgb(l: number, c: number, hDeg: number): Rgb {
    const h = (hDeg * Math.PI) / 180;
    const a = c * Math.cos(h);
    const b = c * Math.sin(h);
    const l_ = (l + 0.3963377774 * a + 0.2158037573 * b) ** 3;
    const m_ = (l - 0.1055613458 * a - 0.0638541728 * b) ** 3;
    const s_ = (l - 0.0894841775 * a - 1.291485548 * b) ** 3;
    const linear = [
        4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
        -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
        -0.0041960863 * l_ - 0.7034186147 * m_ + 1.707614701 * s_,
    ];
    return linear.map((v) => {
        const x = Math.min(1, Math.max(0, v));
        const encoded = x <= 0.0031308 ? 12.92 * x : 1.055 * x ** (1 / 2.4) - 0.055;
        return Math.round(encoded * 255);
    }) as Rgb;
}

function luminance([r, g, b]: Rgb): number {
    const [lr, lg, lb] = [r, g, b].map((v) => {
        const s = v / 255;
        return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
    });
    return 0.2126 * lr + 0.7152 * lg + 0.0722 * lb;
}

function contrast(fg: Rgb, bg: Rgb): number {
    const [hi, lo] = [luminance(fg), luminance(bg)].sort((x, y) => y - x);
    return (hi + 0.05) / (lo + 0.05);
}

/** `bg-<token>/<alpha>` painted over `base`, as the browser composites it. */
function tint(color: Rgb, alpha: number, base: Rgb): Rgb {
    return color.map((v, i) => Math.round(v * alpha + base[i] * (1 - alpha))) as Rgb;
}

function parseOklch(source: string, what: string): Rgb {
    const m = source.match(/oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)/);
    if (!m) throw new Error(`no oklch() colour for ${what}`);
    return oklchToRgb(Number(m[1]), Number(m[2]), Number(m[3]));
}

const config = readFileSync(resolve(process.cwd(), 'tailwind.config.js'), 'utf8');

function token(name: string): Rgb {
    const line = config.split('\n').find((l) => new RegExp(`^\\s*'?${name}'?:\\s*'oklch`).test(l));
    if (!line) throw new Error(`token ${name} not found in tailwind.config.js`);
    return parseOklch(line, name);
}

const WHITE: Rgb = [255, 255, 255];
const AA = 4.5;

describe('design token contrast (WCAG AA, 4.5:1 for body text)', () => {
    const paper = token('paper');
    const surface = token('surface');
    const primary50 = token('50');

    it.each(['ink', 'muted', 'faint'])('%s text is readable on every neutral ground', (name) => {
        const fg = token(name);
        for (const bg of [WHITE, surface, paper, primary50]) {
            expect(contrast(fg, bg)).toBeGreaterThanOrEqual(AA);
        }
    });

    it.each([
        ['success', [0.05, 0.1]],
        ['danger', [0.05, 0.1]],
        ['warning', [0.1]],
    ] as const)('%s text is readable on white and on its own tints', (name, alphas) => {
        const fg = token(name);
        for (const bg of [WHITE, surface, paper]) {
            expect(contrast(fg, bg)).toBeGreaterThanOrEqual(AA);
        }
        for (const alpha of alphas) {
            for (const base of [surface, paper]) {
                expect(contrast(fg, tint(fg, alpha, base))).toBeGreaterThanOrEqual(AA);
            }
        }
    });

    it.each(['success', 'danger', 'warning'])('white text is readable on a solid %s fill', (name) => {
        expect(contrast(WHITE, token(name))).toBeGreaterThanOrEqual(AA);
    });

    it('placeholder text is readable on the white field background', () => {
        const css = readFileSync(resolve(process.cwd(), 'src/index.css'), 'utf8');
        const rule = css.match(/input::placeholder[^{]*\{([^}]*)\}/);
        expect(rule).not.toBeNull();
        expect(contrast(parseOklch(rule![1], 'placeholder'), WHITE)).toBeGreaterThanOrEqual(AA);
    });
});
