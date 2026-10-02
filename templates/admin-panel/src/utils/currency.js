/**
 * Centralized currency formatting utility.
 *
 * Currently supports '$' and 'USD' (normalized to '$').
 * Formats values with thousand separators and two decimal places,
 * with optional compaction (e.g., $14.82m).
 *
 * @param {number|string} value - Numeric value to format
 * @param {string} symbol - Currency symbol or code (e.g. '$', 'USD')
 * @param {object} [options]
 * @param {boolean} [options.compact=false] - Whether to compact the number (e.g. $14.82m)
 * @returns {string} Formatted currency string
 */
export function formatCurrency(value, symbol = '$', { compact = false } = {}) {
  if (value === null || value === undefined || value === '') {
    return '—';
  }

  const num = Number(value);
  if (Number.isNaN(num)) {
    return String(value);
  }

  // Normalize symbol: currently supporting '$' and 'USD'
  const normalizedInput = typeof symbol === 'string' ? symbol.trim().toUpperCase() : '$';
  let currencyPrefix = '$';
  if (normalizedInput === '$' || normalizedInput === 'USD') {
    currencyPrefix = '$';
  } else {
    currencyPrefix = typeof symbol === 'string' && symbol.trim() ? symbol.trim() : '$';
  }

  const isNegative = num < 0;
  const absNum = Math.abs(num);

  if (compact) {
    let compacted = '';
    if (absNum >= 1e12) {
      compacted = `${(absNum / 1e12).toFixed(2)}t`;
    } else if (absNum >= 1e9) {
      compacted = `${(absNum / 1e9).toFixed(2)}b`;
    } else if (absNum >= 1e6) {
      compacted = `${(absNum / 1e6).toFixed(2)}m`;
    } else if (absNum >= 1e3) {
      compacted = `${(absNum / 1e3).toFixed(2)}k`;
    } else {
      compacted = absNum.toFixed(2);
    }

    return `${isNegative ? '-' : ''}${currencyPrefix}${compacted}`;
  }

  const formattedNumber = absNum.toLocaleString('en-US', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2
  });

  return `${isNegative ? '-' : ''}${currencyPrefix}${formattedNumber}`;
}

export default {
  formatCurrency
};
