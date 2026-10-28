import { Router } from './router';
import { checkout } from '../application/checkout/checkout';
import { refund } from '../application/refund/refund';
import { PrismaOrderRepo } from '../infra/prisma/order-repo';
import { StripeGateway } from '../infra/stripe/pay-gateway';

export const router = new Router();
router.post('/checkout', (body) => checkout(new PrismaOrderRepo(), new StripeGateway(), body.items));
router.post('/refund', (body) => refund(new StripeGateway(), body.orderId));
