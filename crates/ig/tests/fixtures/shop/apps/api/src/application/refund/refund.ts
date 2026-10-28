import type { PayGateway } from '../ports/pay-gateway';

export async function refund(pay: PayGateway, orderId: string) {
  await pay.refund(orderId);
}
