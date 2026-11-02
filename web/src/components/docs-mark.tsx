export const DOCS_MARK_PATH = "M22 10 V54 M22 10 H34 A22 22 0 0 1 34 54 H22"

export function DocsMark({ className }: { className?: string }) {
  return (
    <svg aria-hidden="true" className={className} viewBox="0 0 64 64">
      <path
        d={DOCS_MARK_PATH}
        fill="none"
        stroke="currentColor"
        strokeWidth="8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}
