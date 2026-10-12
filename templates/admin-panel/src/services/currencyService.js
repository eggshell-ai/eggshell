import currencyUtil, { formatCurrency, formatNumber } from 'utils/currency';

const currencyService = {
  formatCurrency,
  formatNumber,
  ...currencyUtil
};

export { formatCurrency, formatNumber };
export default currencyService;

