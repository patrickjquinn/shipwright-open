<!--
SPDX-FileCopyrightText: 2026 Patrick Quinn
SPDX-License-Identifier: CC-BY-4.0
-->

# Pricing and the platform fee

**DRAFT. Requires legal and commercial review before publication. Nothing here is an offer.** Items marked **[BUSINESS DECISION]** are proposals for Shipwright to confirm; items marked **[DECIDED]** are decided and still need the legal review before publication; items marked **[LEGAL]** need a lawyer's or tax adviser's answer. Each number is a single configuration value, so it can change without code changes.

## How selling works

**You are the seller. [DECIDED]** You sell your paid app through your own merchant-of-record (MoR) account, Paddle for now (Lemon Squeezy later). The buyer pays at your checkout, your MoR is the seller of record and handles VAT on the sale, and the money goes to you. Reef does not take payments for you and does not pay you anything.

What Reef does:

- **Issues licences.** Reef gives you a webhook URL and you give Reef your webhook secret. Your MoR then tells Reef's licence service about every sale, renewal, refund and chargeback, and the licence service issues or revokes the buyer's licence automatically. (Connecting your MoR is done in the licence service's seller registry, which is being built.)
- **Records every sale.** The licence service passes each payment and adjustment on to the developer portal as a ledger entry, with the net amount.
- **Invoices you its platform fee**, monthly.

## Joining

- **No registration fee. [DECIDED]** Opening a developer account, uploading builds, review and publication cost nothing, for free and paid apps alike. The only charge is the platform fee on paid sales, below.

## Billing details

Before you can set a paid price, give Reef the details to invoice: `PUT /v1/me/billing` with your billing `name`, billing `email`, `country` (ISO 3166-1 alpha-2, such as `DE`) and, if you are a business with one, your `vat_id` (with its country prefix, such as `DE123456789`). `GET /v1/me` shows them. Free apps need none.

## Prices

- An app is `free`, `one_off` (a perpetual licence), or `subscription` (`month` or `year`).
- You set a price with `PUT /v1/apps/{app}/price` in one of EUR, USD, GBP, CHF, SEK, NOK, DKK, PLN, CZK, together with `purchase_url`: your own checkout link at your MoR, an `https://` URL of at most 512 characters. The Reef store sends buyers there. A free app has no `purchase_url`.
- Price range: 0.99 to 999.99 in the chosen currency. **[BUSINESS DECISION]**
- The price you set **excludes VAT**. Your MoR adds VAT or sales tax at checkout where it applies, so buyers in different countries may pay different gross prices. **[BUSINESS DECISION]**: the alternative is tax-inclusive pricing, where your net varies by country instead.
- Keep the price you set here and the price at your checkout the same; Reef shows yours in the store. The app's listing changes from free to paid at the next repository publish.

## The platform fee

For every sale the ledger records the **net amount**: what the buyer paid after discounts and excluding VAT, as your MoR reports it. VAT on the sale never enters the ledger, because your MoR collects it and pays it to the tax authorities.

- **Platform fee: 10% of the net amount. [DECIDED]** It covers licensing, hosting, review and distribution. Your MoR's own fee is between you and your MoR; Reef's fee is on the net amount before it. (Comparable programmes charge 15% to 30%. 10% was chosen to make the store attainable for developers.)
- The fee is computed per sale, in integer cents, as `net × 10%` rounded half to even in cents (49.9 cents becomes 50; 49.4 becomes 49; exactly half goes to the even cent: 28.5 becomes 28, 29.5 becomes 30, 2.5 becomes 2). Every amount is an integer number of the currency's minor unit. Refunds are rounded the same way on their size.
- The fee rate applied is the one in force when the sale happened. A refund or chargeback of that sale uses the same rate, so a full refund cancels exactly the fee its sale added.

Example: a one-off app at 4.99 EUR (net). A buyer in Germany pays 5.94 EUR including 19% VAT at your checkout; your MoR keeps its fee, pays the 0.95 EUR VAT to the tax authority and pays you the rest. Reef's fee for the sale is 0.50 EUR (49.9 cents, rounded), on its monthly invoice to you.

## Refunds and chargebacks

- A refund (full or partial) or a chargeback reduces the fee you owe by the fee on the refunded net amount.
- A chargeback reversed in your favour adds that fee back.
- Refunds and chargebacks are recorded in the month they happen, not the month of the sale. If that month's statement is already closed, they appear in the next open month.
- A refund of a subscription payment revokes that licence; a cancelled subscription keeps working until the end of its paid period.

## Statements and invoices

- One statement per calendar month (UTC) and per currency, available with `GET /v1/me/statements` after the month is closed (normally in the first days of the next month).
- A statement shows the fee owed carried from earlier months (`opening_owed_cents`), sales, refunds and chargebacks, this month's platform fee (`platform_fee_cents`), the closing balance owed (`closing_owed_cents`) and what Reef invoices you (`invoiced_cents`). It also shows the net you kept after the fee (`developer_share_cents`), for information only: Reef pays nothing out.
- **Minimum invoice: 10.00 in the statement's currency. [DECIDED]** A balance owed of exactly 10.00 is invoiced. A smaller balance is carried to the next month (`carry_reason` `below_minimum`).
- A negative balance (more refunded than sold) is a credit: it is carried to the next month (`carry_reason` `credit`) and reduces the next fees. It is never paid out.
- Invoices are issued in the statement's currency and are payable within 30 days. **[BUSINESS DECISION]**, including how you pay and who bears bank and FX fees.
- **VAT on Reef's fee. [LEGAL]** Reef's fee is a service Shipwright supplies to you from Ireland. For a business in another EU country with a valid VAT id, the invoice is issued without VAT under the reverse charge; otherwise Irish VAT may apply.
- You are responsible for your own income tax and for the tax registrations your sales require; your MoR handles VAT on the sales themselves.

## The ledger is itemised

`GET /v1/me/ledger?period=YYYY-MM` lists every entry: the MoR transaction id, type (`sale`, `refund`, `chargeback`, `chargeback_reversal`), the time, currency, net amount, fee rate, fee and the net you kept. It never contains buyer personal data.

## Open questions for review

1. The developer agreement for this model: Reef as a licensing, listing and distribution service invoicing a fee, not a seller or agent in the sale; and what the MoR's terms require of you for selling through a third-party store.
2. VAT on the fee (see above): the reverse-charge wording on invoices, VAT id validation (VIES), and the treatment of developers outside the EU and of EU consumers without a VAT id.
3. Invoice payment: method (card or direct debit at Reef's own billing provider, or bank transfer), terms, currency conversion, and what happens to an unpaid invoice (for example, paid apps hidden until it is settled).
4. Reconciliation: the fee depends on the sales your MoR reports to Reef; how a missed webhook or a disputed amount is corrected.
5. Handling of subscriptions when a developer leaves the programme or an app is withdrawn (refund policy for remaining periods).
6. Free trials and introductory prices (not supported yet).
