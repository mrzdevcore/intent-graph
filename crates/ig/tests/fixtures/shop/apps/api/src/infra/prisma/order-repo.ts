import type { OrderRepo } from '../../application/ports/order-repo';
import type { Order } from '../../domain/order/order';
import { prisma } from './client';

export class PrismaOrderRepo implements OrderRepo {
  async save(order: Order) {
    await prisma.order.create({ data: order });
  }
}
