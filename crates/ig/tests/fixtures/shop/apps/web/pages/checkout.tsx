import { request } from '@shop/api-client';

export default function CheckoutPage() {
  const pay = () => request('/checkout', { method: 'POST' });
  return <button onClick={pay}>Pay</button>;
}
