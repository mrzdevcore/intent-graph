// Deliberate onion violation: the domain reaches into infrastructure.
import { prisma } from '../../infra/prisma/client';

export class Order {
  constructor(public id: string, public amount: number) {}
}

export function loadOrder(id: string) {
  return prisma.order.findUnique({ where: { id } });
}
