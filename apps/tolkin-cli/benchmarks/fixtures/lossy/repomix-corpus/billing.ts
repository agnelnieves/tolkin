// Self-authored fixture for the lossy/repomix comparison row.
// See auth.ts header for provenance.

import Stripe from "stripe";
import { db } from "./db";
import { logger } from "./logger";

const stripe = new Stripe(process.env.STRIPE_SECRET_KEY ?? "");

export interface Invoice {
  id: string;
  customerId: string;
  amountCents: number;
  currency: string;
  status: "draft" | "open" | "paid" | "void";
  dueDate: Date;
}

/**
 * Create a new invoice for a customer.
 * Charges the default payment method when status is "open".
 */
export async function createInvoice(
  customerId: string,
  amountCents: number,
  currency: string,
  autoCharge: boolean,
): Promise<Invoice> {
  const customer = await db.customers.findFirst({ where: { id: customerId } });
  if (!customer) throw new Error(`unknown customer ${customerId}`);
  const invoice = await db.invoices.create({
    data: {
      customerId,
      amountCents,
      currency,
      status: autoCharge ? "open" : "draft",
      dueDate: new Date(Date.now() + 30 * 24 * 60 * 60 * 1000),
    },
  });
  if (autoCharge) {
    try {
      await chargeInvoice(invoice.id);
    } catch (err) {
      logger.error("auto-charge failed", { invoiceId: invoice.id, error: String(err) });
    }
  }
  return invoice;
}

export async function chargeInvoice(invoiceId: string): Promise<void> {
  const invoice = await db.invoices.findFirst({ where: { id: invoiceId } });
  if (!invoice) throw new Error(`unknown invoice ${invoiceId}`);
  if (invoice.status !== "open") {
    throw new Error(`cannot charge invoice in status ${invoice.status}`);
  }
  const customer = await db.customers.findFirst({ where: { id: invoice.customerId } });
  if (!customer?.stripeCustomerId) {
    throw new Error(`customer ${invoice.customerId} has no stripe customer id`);
  }
  const intent = await stripe.paymentIntents.create({
    amount: invoice.amountCents,
    currency: invoice.currency,
    customer: customer.stripeCustomerId,
    confirm: true,
    off_session: true,
    metadata: { invoiceId: invoice.id },
  });
  if (intent.status === "succeeded") {
    await db.invoices.update({
      where: { id: invoice.id },
      data: { status: "paid", paidAt: new Date() },
    });
    logger.info("invoice paid", { invoiceId: invoice.id });
  } else {
    throw new Error(`payment intent ${intent.id} in unexpected status ${intent.status}`);
  }
}

export async function voidInvoice(invoiceId: string, reason: string): Promise<void> {
  await db.invoices.update({
    where: { id: invoiceId },
    data: { status: "void", voidedAt: new Date(), voidReason: reason },
  });
  logger.info("invoice voided", { invoiceId, reason });
}

export async function listInvoicesForCustomer(
  customerId: string,
  status?: Invoice["status"],
): Promise<Invoice[]> {
  const where: { customerId: string; status?: Invoice["status"] } = { customerId };
  if (status) where.status = status;
  const rows = await db.invoices.findMany({ where, orderBy: { dueDate: "desc" } });
  return rows.map((row) => ({
    id: row.id,
    customerId: row.customerId,
    amountCents: row.amountCents,
    currency: row.currency,
    status: row.status,
    dueDate: row.dueDate,
  }));
}
