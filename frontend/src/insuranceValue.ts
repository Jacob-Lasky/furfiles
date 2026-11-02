// Wording + bar geometry for insurance value (#74). Pure, so the copy is unit
// tested once and shared by the policy sheet and the Costs card. The numbers
// themselves come from the API (GET /insurance/:pid/value and friends); this
// module only says them plainly — no "worth it" verdict, that's the person's
// call.
import { money, longDate } from './format'
import type { BreakEven, PolicyValue, Preventative } from './types'

/** Paid back as a share of paid in, for the covered-vs-out-of-pocket bar idiom.
 *  `back` is the green share, `rest` the part of the premium not (yet) returned.
 *  Null when there's nothing to draw (no premium known and nothing paid back). */
export function valueBar(paidIn: number | null | undefined, paidBack: number): { back: number; rest: number } | null {
  if (!paidIn) return paidBack > 0 ? { back: 100, rest: 0 } : null
  const back = Math.min(100, Math.round((paidBack / paidIn) * 100))
  return { back, rest: 100 - back }
}

/** "$507.24 more paid in than back" / "$122.44 more back than paid in so far". */
export function netSentence(net: number | null | undefined, soFar = false): string {
  if (net == null) return ''
  const tail = soFar ? ' so far' : ''
  if (net > 0) return `${money(net)} more back than paid in${tail}`
  if (net < 0) return `${money(-net)} more paid in than back${tail}`
  return `Paid back exactly what was paid in${tail}`
}

/** The break-even, with its inputs shown so the number can be checked. */
export function breakEvenSentence(be: BreakEven): { lead: string; formula: string } {
  const formula = `${money(be.deductible_cents)} deductible + ${money(be.premium_cents)} ÷ ${be.reimburse_percent}%`
  if (be.reachable && be.bill_cents != null) {
    return { lead: `One eligible bill of ${money(be.bill_cents)} would, on its own, return this term's premium.`, formula }
  }
  if (be.reimburse_percent <= 0) {
    return { lead: 'At 0% reimbursement, no bill returns the premium.', formula: '' }
  }
  return {
    lead: `The ${money(be.annual_limit_cents)} annual limit is below the ${money(be.premium_cents)} premium, so no single bill returns it.`,
    formula: '',
  }
}

/** "a, b and c". */
function listJoin(xs: string[]): string {
  return xs.length < 2 ? xs.join('') : `${xs.slice(0, -1).join(', ')} and ${xs[xs.length - 1]}`
}

/** What the policy doesn't record yet, as one gentle ask. Empty when complete. */
export function missingSentence(missing: string[] | null | undefined): string {
  if (!missing || !missing.length) return ''
  return `Add the ${listJoin(missing)} to this policy to fill in the rest.`
}

/** Defaults the numbers leaned on, so an assumed renewal date isn't a surprise. */
export function assumedSentence(v: Pick<PolicyValue, 'assumed' | 'end_date'>): string {
  const parts = (v.assumed || []).map((a) =>
    a === 'end date' ? `it renews ${longDate(v.end_date) || 'a year after it started'}` : 'the premium is per year')
  return parts.length ? `Assuming ${listJoin(parts)}.` : ''
}

/** The preventative nudge. Only for a current or upcoming term — an ended
 *  term's allowance can't be used any more, so it's just stated. It goes by
 *  RECORDED payouts (a claim not imported yet isn't known), and says so; the
 *  API leaves usage unknown for a wellness plan, which pays at the counter. */
export function preventativeSentence(p: Preventative, phase: string): string {
  const total = money(p.allowance_cents)
  if (phase !== 'current' && phase !== 'upcoming') return `${total} of preventative allowance for the term.`
  if (p.limits.length && p.limits.every((l) => l.unused_cents != null)) {
    return `No payouts recorded this term, so all ${total} of preventative allowance looks unused.`
  }
  if (p.unused_at_least_cents > 0) {
    return `Going by recorded payouts, at least ${money(p.unused_at_least_cents)} of the ${total} preventative allowance is unused this term.`
  }
  return `${total} of preventative allowance this term.`
}

/** "This term" / "Starts Sep 14, 2027" / "Sep 14, 2025 → Sep 14, 2026". */
export function termLabel(v: Pick<PolicyValue, 'phase' | 'start_date' | 'end_date'>): string {
  if (v.phase === 'current') return 'This term'
  if (v.phase === 'upcoming') return `Starts ${longDate(v.start_date)}`
  if (!v.start_date) return 'Term dates not recorded'
  return `${longDate(v.start_date)} → ${longDate(v.end_date) || '—'}`
}
