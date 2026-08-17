import { lazy, Suspense } from 'react';

interface QrCodeProps {
  value: string;
  /** Rendered edge length in px. */
  size?: number;
  className?: string;
  /** Shown instead of the code if encoding fails. */
  fallback?: string;
}

/**
 * QR renderer for the Claude Remote Control session link.
 *
 * The encoder (`uqr`) is dynamically imported so it costs nothing on the main
 * bundle until a session is actually running.
 *
 * We render the matrix as SVG ourselves rather than using a raster helper: the
 * modules use `currentColor` on an explicit white quiet-zone plate, which keeps
 * the code scannable in both light and dark themes. A baked-in black-on-white
 * PNG would be unreadable against the dark panel background.
 */
const QrCodeInner = lazy(async () => {
  const { encode } = await import('uqr');

  return {
    default: ({ value, size = 176, className, fallback }: QrCodeProps) => {
      let result;
      try {
        result = encode(value, { ecc: 'M', border: 1 });
      } catch {
        return (
          <p className="max-w-[176px] p-2 text-xs text-neutral-600">
            {fallback}
          </p>
        );
      }

      const { size: modules, data } = result;
      const paths: string[] = [];
      for (let y = 0; y < modules; y++) {
        for (let x = 0; x < modules; x++) {
          if (data[y][x]) paths.push(`M${x} ${y}h1v1h-1z`);
        }
      }

      return (
        <svg
          width={size}
          height={size}
          viewBox={`0 0 ${modules} ${modules}`}
          shapeRendering="crispEdges"
          role="img"
          className={className}
        >
          <rect width={modules} height={modules} fill="#ffffff" />
          <path d={paths.join('')} fill="currentColor" />
        </svg>
      );
    },
  };
});

export function QrCode({
  value,
  size = 176,
  className,
  fallback,
}: QrCodeProps) {
  return (
    <div className="inline-block rounded-sm bg-white p-2 text-black">
      <Suspense
        fallback={
          <div
            style={{ width: size, height: size }}
            className="animate-pulse bg-neutral-200"
          />
        }
      >
        <QrCodeInner
          value={value}
          size={size}
          className={className}
          fallback={fallback}
        />
      </Suspense>
    </div>
  );
}
