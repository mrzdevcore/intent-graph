import type { Order } from '../../domain/order/order';

export interface OrderRepo {
  save(order: Order): Promise<void>;
}
