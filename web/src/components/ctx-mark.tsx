export function CtxMark({ className }: { className?: string }) {
  return (
    <svg aria-hidden="true" className={className} viewBox="0 0 64 64">
      <path
        d="M12.5 12.5 L51.5 51.5 M51.5 12.5 L12.5 51.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="9"
        strokeLinecap="round"
      />
    </svg>
  )
}
