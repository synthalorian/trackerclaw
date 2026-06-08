use crate::store::Store;
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::Path;

#[derive(Debug)]
pub struct InvoiceEntry {
    pub date: String,
    pub description: String,
    pub tags: String,
    pub hours: f64,
    pub rate: f64,
    pub amount: f64,
}

#[derive(Debug)]
pub struct Invoice {
    pub client: String,
    pub invoice_date: String,
    pub period_start: String,
    pub period_end: String,
    pub entries: Vec<InvoiceEntry>,
    pub total_hours: f64,
    pub total_amount: f64,
    pub rate: f64,
}

pub fn generate_invoice(
    db: &str,
    client: &str,
    rate: f64,
    days: i64,
    tag: Option<&str>,
) -> Result<Invoice> {
    let store = Store::open(Path::new(db))?;
    let entries = store.invoice_entries(days, tag)?;

    let mut invoice_entries = Vec::new();
    let mut total_hours = 0.0;

    for e in entries {
        let hours = e.duration_seconds.unwrap_or(0) as f64 / 3600.0;
        let amount = hours * rate;
        let date = DateTime::parse_from_rfc3339(&e.started_at)
            .map(|d| d.with_timezone(&Utc).format("%Y-%m-%d").to_string())
            .unwrap_or_else(|_| e.started_at.clone());

        invoice_entries.push(InvoiceEntry {
            date,
            description: e.name,
            tags: e.tags.unwrap_or_default(),
            hours,
            rate,
            amount,
        });
        total_hours += hours;
    }

    let period_end = Utc::now().format("%Y-%m-%d").to_string();
    let period_start = (Utc::now() - chrono::Duration::days(days)).format("%Y-%m-%d").to_string();

    Ok(Invoice {
        client: client.to_string(),
        invoice_date: Utc::now().format("%Y-%m-%d").to_string(),
        period_start,
        period_end,
        total_hours,
        total_amount: total_hours * rate,
        rate,
        entries: invoice_entries,
    })
}

pub fn render_html_invoice(invoice: &Invoice) -> String {
    let mut rows = String::new();
    for entry in &invoice.entries {
        rows.push_str(&format!(
            r#"<tr>
                <td>{}</td>
                <td>{}</td>
                <td>{}</td>
                <td>{:.2}</td>
                <td>${:.2}</td>
                <td>${:.2}</td>
            </tr>"#,
            entry.date, entry.description, entry.tags, entry.hours, entry.rate, entry.amount
        ));
    }

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Invoice — {}</title>
<style>
    * {{ margin: 0; padding: 0; box-sizing: border-box; }}
    body {{
        background: linear-gradient(135deg, #0a0a12 0%, #1a0a2e 50%, #0a0a12 100%);
        color: #e0e0e0;
        font-family: 'Segoe UI', system-ui, sans-serif;
        padding: 2rem;
        min-height: 100vh;
    }}
    .invoice {{
        max-width: 900px;
        margin: 0 auto;
        background: rgba(20, 20, 40, 0.95);
        border: 1px solid #333;
        border-radius: 12px;
        padding: 3rem;
        box-shadow: 0 0 40px rgba(0, 255, 255, 0.1);
    }}
    .header {{
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        margin-bottom: 2rem;
        padding-bottom: 2rem;
        border-bottom: 2px solid #00ffff;
    }}
    .logo {{
        font-size: 2.5rem;
        font-weight: bold;
        color: #00ffff;
        text-shadow: 0 0 20px rgba(0, 255, 255, 0.5);
    }}
    .invoice-meta {{
        text-align: right;
        color: #888;
    }}
    .invoice-meta .label {{
        color: #ff00ff;
        font-size: 0.85rem;
        text-transform: uppercase;
        letter-spacing: 1px;
    }}
    .invoice-meta .value {{
        color: #e0e0e0;
        font-size: 1.1rem;
        margin-bottom: 0.5rem;
    }}
    .client-info {{
        margin-bottom: 2rem;
    }}
    .client-info .label {{
        color: #ff00ff;
        font-size: 0.85rem;
        text-transform: uppercase;
        letter-spacing: 1px;
        margin-bottom: 0.5rem;
    }}
    .client-info .name {{
        font-size: 1.5rem;
        color: #ffff00;
    }}
    table {{
        width: 100%;
        border-collapse: collapse;
        margin: 2rem 0;
    }}
    th {{
        background: linear-gradient(90deg, #00ffff22, #ff00ff22);
        color: #00ffff;
        padding: 1rem;
        text-align: left;
        font-size: 0.85rem;
        text-transform: uppercase;
        letter-spacing: 1px;
        border-bottom: 2px solid #00ffff;
    }}
    td {{
        padding: 0.85rem 1rem;
        border-bottom: 1px solid #222;
        color: #ccc;
    }}
    tr:hover td {{
        background: rgba(0, 255, 255, 0.05);
    }}
    .total-row {{
        background: linear-gradient(90deg, #00ffff11, #ff00ff11);
    }}
    .total-row td {{
        color: #fff;
        font-weight: bold;
        font-size: 1.1rem;
        border-top: 2px solid #ff00ff;
    }}
    .summary {{
        margin-top: 2rem;
        padding-top: 2rem;
        border-top: 1px solid #333;
        display: flex;
        justify-content: space-between;
    }}
    .summary-item .label {{
        color: #888;
        font-size: 0.85rem;
    }}
    .summary-item .value {{
        color: #00ffff;
        font-size: 1.3rem;
        font-weight: bold;
    }}
    .footer {{
        margin-top: 3rem;
        text-align: center;
        color: #555;
        font-size: 0.85rem;
    }}
    .neon-text {{
        color: #ff00ff;
        text-shadow: 0 0 10px rgba(255, 0, 255, 0.5);
    }}
    @media print {{
        body {{ background: #fff; color: #000; }}
        .invoice {{ box-shadow: none; border: 1px solid #ccc; }}
    }}
</style>
</head>
<body>
<div class="invoice">
    <div class="header">
        <div class="logo">⏱️ OPENTRACKER</div>
        <div class="invoice-meta">
            <div class="label">Invoice #</div>
            <div class="value">INV-{}</div>
            <div class="label">Date</div>
            <div class="value">{}</div>
            <div class="label">Period</div>
            <div class="value">{} — {}</div>
        </div>
    </div>

    <div class="client-info">
        <div class="label">Bill To</div>
        <div class="name">{}</div>
    </div>

    <table>
        <thead>
            <tr>
                <th>Date</th>
                <th>Description</th>
                <th>Tags</th>
                <th>Hours</th>
                <th>Rate</th>
                <th>Amount</th>
            </tr>
        </thead>
        <tbody>
            {}
            <tr class="total-row">
                <td colspan="3"></td>
                <td>{:.2}h</td>
                <td></td>
                <td class="neon-text">${:.2}</td>
            </tr>
        </tbody>
    </table>

    <div class="summary">
        <div class="summary-item">
            <div class="label">Total Hours</div>
            <div class="value">{:.2}h</div>
        </div>
        <div class="summary-item">
            <div class="label">Rate</div>
            <div class="value">${:.2}/hr</div>
        </div>
        <div class="summary-item">
            <div class="label">Total Due</div>
            <div class="value neon-text">${:.2}</div>
        </div>
    </div>

    <div class="footer">
        <p>Generated by OpenTracker — Privacy-first time tracking</p>
        <p>Payment due within 30 days. Thank you for your business!</p>
    </div>
</div>
</body>
</html>"#,
        invoice.client,
        invoice.invoice_date.replace("-", ""),
        invoice.invoice_date,
        invoice.period_start,
        invoice.period_end,
        invoice.client,
        rows,
        invoice.total_hours,
        invoice.total_amount,
        invoice.total_hours,
        invoice.rate,
        invoice.total_amount
    )
}

pub fn render_markdown_invoice(invoice: &Invoice) -> String {
    let mut lines = Vec::new();
    lines.push(format!("# Invoice — {}", invoice.client));
    lines.push(String::new());
    lines.push(format!("- **Invoice Date:** {}", invoice.invoice_date));
    lines.push(format!("- **Period:** {} — {}", invoice.period_start, invoice.period_end));
    lines.push(String::new());
    lines.push(format!("## Bill To\n\n{}", invoice.client));
    lines.push(String::new());
    lines.push("| Date | Description | Tags | Hours | Rate | Amount |".to_string());
    lines.push("|------|-------------|------|-------|------|--------|".to_string());

    for entry in &invoice.entries {
        lines.push(format!(
            "| {} | {} | {} | {:.2} | ${:.2} | ${:.2} |",
            entry.date, entry.description, entry.tags, entry.hours, entry.rate, entry.amount
        ));
    }

    lines.push(format!(
        "| | | | **{:.2}h** | | **${:.2}** |",
        invoice.total_hours, invoice.total_amount
    ));

    lines.push(String::new());
    lines.push("## Summary".to_string());
    lines.push(String::new());
    lines.push(format!("- **Total Hours:** {:.2}h", invoice.total_hours));
    lines.push(format!("- **Rate:** ${:.2}/hr", invoice.rate));
    lines.push(format!("- **Total Due:** ${:.2}", invoice.total_amount));
    lines.push(String::new());
    lines.push("---".to_string());
    lines.push("Generated by OpenTracker — Privacy-first time tracking".to_string());

    lines.join("\n")
}

pub async fn generate_invoice_file(
    db: &str,
    client: &str,
    rate: f64,
    days: i64,
    tag: Option<&str>,
    output: &std::path::Path,
) -> Result<()> {
    let invoice = generate_invoice(db, client, rate, days, tag)?;
    let is_markdown = output.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
        .unwrap_or(false);

    let content = if is_markdown {
        render_markdown_invoice(&invoice)
    } else {
        render_html_invoice(&invoice)
    };

    std::fs::write(output, content)?;
    println!("Invoice generated: {}", output.display());
    println!("  Client: {}", invoice.client);
    println!("  Period: {} — {}", invoice.period_start, invoice.period_end);
    println!("  Hours:  {:.2}h", invoice.total_hours);
    println!("  Total:  ${:.2}", invoice.total_amount);
    Ok(())
}
