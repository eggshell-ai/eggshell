import currencyUtil, { formatCurrency } from 'utils/currency';

const currencyService = {
  formatCurrency,
  ...currencyUtil
};

export { formatCurrency };
export default currencyService;
