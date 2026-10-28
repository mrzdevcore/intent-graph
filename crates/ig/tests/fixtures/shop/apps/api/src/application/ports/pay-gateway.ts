export interface PayGateway {
  charge(amount: number): Promise<string>;
  refund(id: string): Promise<void>;
}
