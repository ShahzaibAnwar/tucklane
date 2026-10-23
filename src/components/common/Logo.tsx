export function LogoMark({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 1024 1024" aria-hidden>
      <rect width="1024" height="1024" rx="232" fill="#123D38" />
      <path
        d="M196 330a56 56 0 0 1 56-56h178a56 56 0 0 1 42 19l44 49h264a56 56 0 0 1 56 56v58H196z"
        fill="#2B7A6A"
      />
      <rect x="262" y="356" width="500" height="120" rx="22" fill="#CFEFE4" />
      <rect x="300" y="392" width="424" height="120" rx="22" fill="#F7F9F8" />
      <path
        d="M172 470a56 56 0 0 1 56-56h568a56 56 0 0 1 56 56v248a72 72 0 0 1-72 72H244a72 72 0 0 1-72-72z"
        fill="#50C7A3"
      />
      <path
        d="M396 600l84 84 162-170"
        fill="none"
        stroke="#123D38"
        strokeWidth="68"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
