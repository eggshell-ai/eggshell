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
 * @param {number} [options.decimals] - Number of decimal places
 * @returns {string} Formatted currency string
 */
export function formatCurrency(value, symbol = '$', { compact = false, decimals } = {}) {
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
  const fractionDigits = decimals !== undefined ? decimals : 2;

  if (compact) {
    let compacted = '';
    if (absNum >= 1e12) {
      compacted = `${(absNum / 1e12).toFixed(fractionDigits)}t`;
    } else if (absNum >= 1e9) {
      compacted = `${(absNum / 1e9).toFixed(fractionDigits)}b`;
    } else if (absNum >= 1e6) {
      compacted = `${(absNum / 1e6).toFixed(fractionDigits)}m`;
    } else if (absNum >= 1e3) {
      compacted = `${(absNum / 1e3).toFixed(fractionDigits)}k`;
    } else {
      compacted = absNum.toFixed(fractionDigits);
    }

    return `${isNegative ? '-' : ''}${currencyPrefix}${compacted}`;
  }

  const formattedNumber = absNum.toLocaleString('en-US', {
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits
  });

  return `${isNegative ? '-' : ''}${currencyPrefix}${formattedNumber}`;
}

/**
 * Centralized number formatting utility.
 * Formats values with thousand separators or optional compaction (e.g. 14.82m).
 *
 * @param {number|string} value - Numeric value to format
 * @param {object} [options]
 * @param {boolean} [options.compact=false] - Whether to compact the number (e.g. 14.82k)
 * @param {number} [options.decimals] - Decimal places
 * @returns {string} Formatted number string
 */
export function formatNumber(value, { compact = false, decimals } = {}) {
  if (value === null || value === undefined || value === '') {
    return '—';
  }

  const num = Number(value);
  if (Number.isNaN(num)) {
    return String(value);
  }

  const isNegative = num < 0;
  const absNum = Math.abs(num);

  if (compact) {
    const fractionDigits = decimals !== undefined ? decimals : 2;
    let compacted = '';
    if (absNum >= 1e12) {
      compacted = `${(absNum / 1e12).toFixed(fractionDigits)}t`;
    } else if (absNum >= 1e9) {
      compacted = `${(absNum / 1e9).toFixed(fractionDigits)}b`;
    } else if (absNum >= 1e6) {
      compacted = `${(absNum / 1e6).toFixed(fractionDigits)}m`;
    } else if (absNum >= 1e3) {
      compacted = `${(absNum / 1e3).toFixed(fractionDigits)}k`;
    } else {
      compacted = decimals !== undefined ? absNum.toFixed(fractionDigits) : absNum.toLocaleString('en-US');
    }

    return `${isNegative ? '-' : ''}${compacted}`;
  }

  const fractionOptions =
    decimals !== undefined
      ? { minimumFractionDigits: decimals, maximumFractionDigits: decimals }
      : {};

  const formattedNumber = absNum.toLocaleString('en-US', fractionOptions);
  return `${isNegative ? '-' : ''}${formattedNumber}`;
}

export default {
  formatCurrency,
  formatNumber
};
