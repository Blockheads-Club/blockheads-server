/** Layout by width (see "Layouts by width" in the design): phone < 600, rail 600-1023, sidebar 1024+. */
export const viewport = $state({ phone: false, rail: false });

/** Track the media queries; returns a cleanup function. */
export function watchViewport(): () => void {
  const phone = window.matchMedia('(max-width: 599px)');
  const rail = window.matchMedia('(min-width: 600px) and (max-width: 1023px)');
  const sync = () => {
    viewport.phone = phone.matches;
    viewport.rail = rail.matches;
  };
  sync();
  phone.addEventListener('change', sync);
  rail.addEventListener('change', sync);
  return () => {
    phone.removeEventListener('change', sync);
    rail.removeEventListener('change', sync);
  };
}
