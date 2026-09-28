import { getContext, setContext } from 'svelte';

const CURRENCY_NAME_CONTEXT = Symbol('bblbb.currency-name');

export interface CurrencyNameContext {
  currencyName: string;
}

/** Provide request/page-scoped site currency; never store SSR values in a process-global singleton. */
export function setCurrencyNameContext(context: CurrencyNameContext): void {
  setContext(CURRENCY_NAME_CONTEXT, context);
}

/** Read the nearest layout-provided reactive currency name. */
export function getCurrencyNameContext(): CurrencyNameContext | undefined {
  return getContext<CurrencyNameContext | undefined>(CURRENCY_NAME_CONTEXT);
}
