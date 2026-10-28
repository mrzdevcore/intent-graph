import type { OrderRepo } from '../ports/order-repo';
import type { PayGateway } from '../ports/pay-gateway';
import type { Order } from '../../domain/order/order';
import { total } from '../../domain/payment/payment';

export async function checkout(orderRepo: OrderRepo, pay: PayGateway, items: Order[]) {
  for (const item of items) await orderRepo.save(item);
  return pay.charge(total(items));
}
