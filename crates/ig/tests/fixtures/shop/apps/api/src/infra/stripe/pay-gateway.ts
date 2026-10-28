import Stripe from 'stripe';
import type { PayGateway } from '../../application/ports/pay-gateway';

const stripe = new Stripe('sk_test');

export class StripeGateway implements PayGateway {
  async charge(amount: number) {
    const intent = await stripe.paymentIntents.create({ amount, currency: 'eur' });
    return intent.id;
  }
  async refund(id: string) {
    await stripe.refunds.create({ payment_intent: id });
  }
}
