/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        // Brand: deep, editorial cobalt (replaces the old washed sky-blue).
        // `primary` is remapped so existing primary-* usages adopt the new brand.
        primary: {
          50:  'oklch(0.971 0.018 262 / <alpha-value>)',
          100: 'oklch(0.936 0.035 262 / <alpha-value>)',
          200: 'oklch(0.882 0.062 262 / <alpha-value>)',
          300: 'oklch(0.806 0.095 262 / <alpha-value>)',
          400: 'oklch(0.674 0.138 262 / <alpha-value>)',
          500: 'oklch(0.566 0.166 262 / <alpha-value>)',
          600: 'oklch(0.502 0.176 263 / <alpha-value>)',
          700: 'oklch(0.444 0.165 264 / <alpha-value>)',
          800: 'oklch(0.388 0.140 266 / <alpha-value>)',
          900: 'oklch(0.330 0.110 268 / <alpha-value>)',
          950: 'oklch(0.248 0.082 269 / <alpha-value>)',
        },
        // accent alias for new code
        accent: {
          DEFAULT: 'oklch(0.502 0.176 263 / <alpha-value>)',
          soft: 'oklch(0.936 0.035 262 / <alpha-value>)',
        },
        // Neutral scale, tinted slightly toward the cobalt hue for cohesion.
        ink:   'oklch(0.235 0.021 266 / <alpha-value>)', // primary text / near-black
        muted: 'oklch(0.450 0.018 266 / <alpha-value>)', // secondary text
        faint: 'oklch(0.598 0.014 266 / <alpha-value>)', // tertiary text
        line:  'oklch(0.916 0.008 266 / <alpha-value>)', // hairline border
        line2: 'oklch(0.852 0.010 266 / <alpha-value>)', // stronger border
        paper: 'oklch(0.986 0.004 266 / <alpha-value>)', // page background
        surface: 'oklch(0.999 0.001 266 / <alpha-value>)', // card surface
        // States (reserved strictly for status, never decoration)
        success: 'oklch(0.560 0.110 158 / <alpha-value>)',
        danger:  'oklch(0.560 0.196 25 / <alpha-value>)',
        warning: 'oklch(0.740 0.140 76 / <alpha-value>)',
      },
      fontFamily: {
        // Distinctive grotesque display + calm humanist body. Mono only for URLs/codes.
        display: ['"Bricolage Grotesque"', 'Georgia', 'serif'],
        sans: ['"Hanken Grotesk"', 'system-ui', '-apple-system', 'sans-serif'],
        mono: ['"JetBrains Mono"', 'ui-monospace', 'monospace'],
      },
      letterSpacing: {
        tightest: '-0.04em',
      },
      borderRadius: {
        '4xl': '2rem',
      },
      boxShadow: {
        // Soft, layered elevation (no colored "glow").
        'subtle': '0 1px 2px oklch(0.235 0.021 266 / 0.04), 0 1px 3px oklch(0.235 0.021 266 / 0.06)',
        'card': '0 2px 4px oklch(0.235 0.021 266 / 0.04), 0 6px 16px oklch(0.235 0.021 266 / 0.06)',
        'lift': '0 8px 24px oklch(0.235 0.021 266 / 0.10), 0 2px 6px oklch(0.235 0.021 266 / 0.06)',
      },
      animation: {
        'fade-in-up': 'fade-in-up 0.5s cubic-bezier(0.16, 1, 0.3, 1) forwards',
      },
    },
  },
  plugins: [],
}
