import { useEffect, useRef } from 'react';

/**
 * Ambient "breathing network" canvas layered over the static hero
 * network graphic. Nodes drift a few pixels on slow sine paths and the
 * links between them fade in and out on long periods — calm, not motion.
 *
 * Engineering constraints (deliberate):
 * - Hand-rolled rAF; no animation library.
 * - Pauses when the tab is hidden or the canvas leaves the viewport.
 * - DPR-aware but capped at 1.5x to bound fill cost.
 * - `prefers-reduced-motion: reduce` renders one static frame, no loop.
 * - Purely decorative: aria-hidden, pointer-events disabled by the caller,
 *   zero layout impact (absolutely positioned by the caller).
 * - If canvas/2D is unavailable (old browsers, jsdom), it renders nothing
 *   and the static PNG underneath carries the design unchanged.
 */

interface Node {
    x: number; // base position, 0..1 relative to canvas size
    y: number;
    ax: number; // drift amplitude in px
    ay: number;
    tx: number; // drift period in seconds
    ty: number;
    phase: number;
    r: number; // dot radius in px
}

interface LinkPair {
    a: number;
    b: number;
    period: number; // opacity cycle in seconds
    phase: number;
}

// Deep cobalt (primary-600) — matches the tailwind token; rgb fallback for
// canvases without oklch() support.
const STROKE_FALLBACK = 'rgb(63, 81, 181)';
const STROKE_OKLCH = 'oklch(0.502 0.176 263)';

function buildField(count: number, rng: () => number): { nodes: Node[]; links: LinkPair[] } {
    const nodes: Node[] = Array.from({ length: count }, () => ({
        // Bias the field toward the top-right, where the static art lives.
        x: 0.25 + rng() * 0.75,
        y: rng() * 0.85,
        ax: 2.5 + rng() * 3.5,
        ay: 2.5 + rng() * 3.5,
        tx: 9 + rng() * 8,
        ty: 9 + rng() * 8,
        phase: rng() * Math.PI * 2,
        r: 1.1 + rng() * 1.1,
    }));

    const links: LinkPair[] = [];
    for (let i = 0; i < nodes.length; i++) {
        for (let j = i + 1; j < nodes.length; j++) {
            const dx = nodes[i].x - nodes[j].x;
            const dy = nodes[i].y - nodes[j].y;
            if (Math.hypot(dx, dy) < 0.22) {
                links.push({ a: i, b: j, period: 12 + rng() * 12, phase: rng() * Math.PI * 2 });
            }
        }
    }
    return { nodes, links };
}

export default function AmbientNetwork({ className }: { className?: string }) {
    const canvasRef = useRef<HTMLCanvasElement | null>(null);

    useEffect(() => {
        const canvas = canvasRef.current;
        const ctx = canvas?.getContext?.('2d');
        if (!canvas || !ctx) return;

        const reduceMotion = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;
        const { nodes, links } = buildField(26, Math.random);

        let width = 0;
        let height = 0;
        let raf = 0;
        let inView = true;
        let disposed = false;

        const resize = () => {
            const rect = canvas.getBoundingClientRect();
            if (rect.width === 0 || rect.height === 0) return;
            const dpr = Math.min(window.devicePixelRatio || 1, 1.5);
            width = rect.width;
            height = rect.height;
            canvas.width = Math.round(rect.width * dpr);
            canvas.height = Math.round(rect.height * dpr);
            ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        };

        const positionAt = (n: Node, t: number) => ({
            x: n.x * width + Math.sin((t / n.tx) * Math.PI * 2 + n.phase) * n.ax,
            y: n.y * height + Math.cos((t / n.ty) * Math.PI * 2 + n.phase) * n.ay,
        });

        const drawFrame = (t: number) => {
            ctx.clearRect(0, 0, width, height);
            ctx.strokeStyle = STROKE_FALLBACK;
            ctx.fillStyle = STROKE_FALLBACK;
            try {
                ctx.strokeStyle = STROKE_OKLCH;
                ctx.fillStyle = STROKE_OKLCH;
            } catch {
                /* keep rgb fallback */
            }
            ctx.lineWidth = 1;

            const pts = nodes.map((n) => positionAt(n, t));

            for (const link of links) {
                const breathe = 0.5 + 0.5 * Math.sin((t / link.period) * Math.PI * 2 + link.phase);
                ctx.globalAlpha = 0.11 * breathe;
                ctx.beginPath();
                ctx.moveTo(pts[link.a].x, pts[link.a].y);
                ctx.lineTo(pts[link.b].x, pts[link.b].y);
                ctx.stroke();
            }

            for (let i = 0; i < nodes.length; i++) {
                const breathe = 0.75 + 0.25 * Math.sin((t / nodes[i].tx) * Math.PI * 2);
                ctx.globalAlpha = 0.30 * breathe;
                ctx.beginPath();
                ctx.arc(pts[i].x, pts[i].y, nodes[i].r, 0, Math.PI * 2);
                ctx.fill();
            }
            ctx.globalAlpha = 1;
        };

        const start = performance.now();
        const loop = () => {
            if (disposed) return;
            drawFrame((performance.now() - start) / 1000);
            raf = requestAnimationFrame(loop);
        };

        const running = () => !disposed && inView && document.visibilityState === 'visible';
        const sync = () => {
            cancelAnimationFrame(raf);
            if (running() && !reduceMotion) raf = requestAnimationFrame(loop);
        };

        resize();
        // Always paint frame 0 immediately: no pop-in when the loop starts,
        // and under reduced motion this single static frame is the design.
        drawFrame(0);
        if (!reduceMotion) sync();

        const ro = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(() => { resize(); if (reduceMotion) drawFrame(0); }) : null;
        ro?.observe(canvas);

        const io = typeof IntersectionObserver !== 'undefined'
            ? new IntersectionObserver((entries) => { inView = entries[0]?.isIntersecting ?? true; sync(); })
            : null;
        io?.observe(canvas);

        const onVisibility = () => sync();
        document.addEventListener('visibilitychange', onVisibility);

        return () => {
            disposed = true;
            cancelAnimationFrame(raf);
            ro?.disconnect();
            io?.disconnect();
            document.removeEventListener('visibilitychange', onVisibility);
        };
    }, []);

    return <canvas ref={canvasRef} className={className} aria-hidden="true" />;
}
