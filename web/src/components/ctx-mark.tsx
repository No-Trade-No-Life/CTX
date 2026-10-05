export const CTX_MARK_PATH = "M12.5 12.5 L51.5 51.5 M51.5 12.5 L12.5 51.5"

export function CtxMark({ className }: { className?: string }) {
  return (
    <svg aria-hidden="true" className={className} viewBox="0 0 64 64">
      <path
        d={CTX_MARK_PATH}
        fill="none"
        stroke="currentColor"
        strokeWidth="9"
        strokeLinecap="round"
      />
    </svg>
  )
}
