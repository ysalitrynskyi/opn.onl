import { useEffect, useRef } from 'react';

interface SparklineProps {
    data: number[];
    width?: number;
    height?: number;
    color?: string;
    fillColor?: string;
    strokeWidth?: number;
    showTooltip?: boolean;
    labels?: string[];
}

export const Sparkline = ({
    data,
    width = 80,
    height = 24,
    color = '#6366f1',
    fillColor = 'rgba(99, 102, 241, 0.1)',
    strokeWidth = 1.5,
    showTooltip = true,
    labels = [],
}: SparklineProps) => {
    const svgRef = useRef<SVGSVGElement>(null);

    if (!data || data.length === 0) {
        return (
            <div 
                className="flex items-center justify-center text-slate-400 text-xs"
                style={{ width, height }}
            >
                No data
            </div>
        );
    }

    const padding = 2;
    const chartWidth = width - padding * 2;
    const chartHeight = height - padding * 2;

    const max = Math.max(...data, 1);
    const min = Math.min(...data, 0);
    const range = max - min || 1;

    const points = data.map((value, index) => {
        const x = padding + (index / (data.length - 1 || 1)) * chartWidth;
        const y = padding + chartHeight - ((value - min) / range) * chartHeight;
        return { x, y, value };
    });

    const linePath = points.map((p, i) => `${i === 0 ? 'M' : 'L'} ${p.x} ${p.y}`).join(' ');
    
    // Create area fill path
    const areaPath = `${linePath} L ${points[points.length - 1].x} ${height - padding} L ${padding} ${height - padding} Z`;

    const total = data.reduce((sum, v) => sum + v, 0);

    return (
        <div className="relative group" title={showTooltip ? `Total: ${total} clicks` : undefined}>
            <svg
                ref={svgRef}
                width={width}
                height={height}
                className="overflow-visible"
            >
                {/* Gradient definition */}
                <defs>
                    <linearGradient id={`sparkline-gradient-${data.join('-')}`} x1="0" y1="0" x2="0" y2="1">
                        <stop offset="0%" stopColor={fillColor} />
                        <stop offset="100%" stopColor="transparent" />
                    </linearGradient>
                </defs>

                {/* Fill area */}
                <path
                    d={areaPath}
                    fill={fillColor}
                    opacity={0.5}
                />

                {/* Line */}
                <path
                    d={linePath}
                    fill="none"
                    stroke={color}
                    strokeWidth={strokeWidth}
                    strokeLinecap="round"
                    strokeLinejoin="round"
                />

                {/* End point dot */}
                {points.length > 0 && (
                    <circle
                        cx={points[points.length - 1].x}
                        cy={points[points.length - 1].y}
                        r={2.5}
                        fill={color}
                    />
                )}
            </svg>

            {/* Tooltip on hover */}
            {showTooltip && (
                <div className="absolute bottom-full left-1/2 transform -translate-x-1/2 mb-1 opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none z-10">
                    <div className="bg-slate-800 text-white text-xs px-2 py-1 rounded shadow-lg whitespace-nowrap">
                        {labels.length > 0 ? (
                            <div className="flex gap-1">
                                {data.map((v, i) => (
                                    <span key={i} className="text-center">
                                        <span className="block text-slate-400 text-[10px]">{labels[i]}</span>
                                        <span className="block font-medium">{v}</span>
                                    </span>
                                ))}
                            </div>
                        ) : (
                            <span>{total} clicks (7d)</span>
                        )}
                    </div>
                </div>
            )}
        </div>
    );
};

export default Sparkline;
