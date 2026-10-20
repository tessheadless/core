---
sidebar_position: 4
title: Forms
---

# Forms

Tess collects the forms of your websites and apps (contact, reservation,
sign-up, survey…) without a form service. Admins define each form in
**Admin → Forms**. Your front-end renders it from its public definition and
posts the answers to Tess. Tess validates and keeps them, and sends a
`form.submitted` event to your [webhooks](webhooks.md), so an
[n8n](n8n.md) workflow can act on every submission.

Forms are headless: Tess serves no HTML. Your front-end controls the
design.

## Defining a form

In **Admin → Forms → New form**:

- **Title** and **slug**: the slug is in the URL, `/api/forms/<slug>`.
- **Fields**, in order. Each has a label, a **name** (the key of the value in
  submissions, e.g. `email`), a type, and whether it is required:

  | Type | Value | Checked |
  |---|---|---|
  | Text, Long text | text | length (1,000 / 10,000 characters by default) |
  | Email, Phone, URL | text | format (URL: `http(s)://…`) |
  | Number | number | min / max |
  | Date | `YYYY-MM-DD` | a valid date |
  | Date and time | `YYYY-MM-DDTHH:MM` | a valid date and time |
  | Choice | one of the options | |
  | Multiple choice | a list of options | |
  | Checkbox | `true` / `false` | a required checkbox must be checked (e.g. "I accept the terms") |

- **Success message**, returned to your front-end after a submission.
- **Redirect URL**, for plain HTML forms (see below).
- **Accept submissions**: a closed form answers `404` and its definition is
  hidden.

## Using a form in a front-end

The definition is public:

```bash
curl https://cms.example.com/api/forms/contact
```

```json
{
  "slug": "contact",
  "title": "Contact us",
  "fields": [
    { "name": "email", "label": "Email", "type": "EMAIL", "required": true },
    { "name": "topic", "label": "Topic", "type": "SELECT", "required": false, "options": ["Sales", "Support"] },
    { "name": "message", "label": "Message", "type": "TEXTAREA", "required": true }
  ],
  "successMessage": "Thanks, we'll get back to you soon.",
  "honeypot": "_hp"
}
```

Post the answers as JSON:

```js
const res = await fetch("https://cms.example.com/api/forms/contact", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ email: "ada@example.com", topic: "Sales", message: "Hello!" }),
});
const body = await res.json();
// 201: { ok: true, id, message }  ·  400: { error, fields: { email: "must be an email address" } }
```

| Answer | Meaning |
|---|---|
| `201` | Stored; `message` is the success message. |
| `400` | Invalid: `fields` has an error message per field, to show next to it. |
| `404` | No open form with this slug. |
| `413` | The body is over 64 KiB. |
| `429` | The visitor posted too often (see the rate limit below); retry after `Retry-After` seconds. |

Unknown keys are ignored, and values are normalized: trimmed text, numbers as
numbers, checkboxes as booleans.

### Plain HTML forms

No JavaScript needed: post the form directly. Repeated names (multiple choice)
become a list. With a **redirect URL**, the browser is sent there after a
successful submission. Otherwise it gets the JSON answer.

```html
<form method="post" action="https://cms.example.com/api/forms/contact">
  <input name="email" type="email" required>
  <select name="topic"><option>Sales</option><option>Support</option></select>
  <textarea name="message" required></textarea>
  <!-- Anti-spam: keep it hidden and empty. -->
  <input name="_hp" hidden tabindex="-1" autocomplete="off">
  <button>Send</button>
</form>
```

Calls from a browser on another origin need that origin in
`TESS_CORS_ORIGINS` (the default `*` allows all).

## Spam

- **Honeypot**: add a hidden field named `_hp` and keep it empty. Bots fill
  it; Tess then answers as usual but drops the submission.
- **Rate limit**: each visitor (IP address) can post
  `TESS_FORMS_RATE_LIMIT` submissions per form per minute (default 10). Behind a
  reverse proxy, Tess reads the visitor's address from `X-Forwarded-For` when
  the connection comes from a private or local address.

## Submissions

**Admin → Forms → Submissions** lists them, newest first, with the page they
were posted from. Select some to delete them, or **Export CSV**. Submissions
contain personal data: export and delete them according to your privacy
policy. Deleting a form deletes its submissions.

They are stored in the settings store of your database, so forms work with
every backend.

## Automations with n8n

Every stored submission emits `form.submitted`:

```json
{
  "event": "form.submitted",
  "data": {
    "form": { "id": "…", "slug": "contact", "title": "Contact us" },
    "submission": { "id": "…", "formId": "…", "data": { "email": "ada@example.com", "message": "Hello!" }, "submittedAt": "…", "source": "https://www.example.com/contact" }
  }
}
```

In n8n, start a workflow with the **Tess Trigger** on `form.submitted`, then
use `{{ $json.data.submission.data.email }}`. For example:

- a **reservation** form → **Tess → Calendar → Create Event** in a linked
  Google calendar, with the visitor as attendee, so Google emails the
  invitation (see [Calendars](calendars.md));
- a **contact** form → send an email or a Slack message, or create a ticket;
- a **sign-up** form → add the person to your newsletter tool.

The **Tess** node also reads submissions (**Form → Get Submissions**).

## GraphQL API (Admins)

```graphql
mutation {
  createForm(input: {
    slug: "contact", title: "Contact us", successMessage: "Thanks!",
    fields: [
      { name: "email", label: "Email", type: EMAIL, required: true }
      { name: "message", label: "Message", type: TEXTAREA, required: true }
    ]
  }) { id }
}

query { forms { id slug title enabled submissionCount } }
query($id: ID!) { formSubmissions(formId: $id, first: 50) { total items { data submittedAt source } } }
mutation($id: ID!, $ids: [ID!]!) { deleteFormSubmissions(formId: $id, ids: $ids) }
```

`updateForm` replaces the definition and keeps the submissions. Renaming a
field leaves the values already submitted under the old name.

## Settings

| Variable | Default | |
|---|---|---|
| `TESS_FORMS` | `true` | `false` turns forms off. |
| `TESS_FORMS_RATE_LIMIT` | `10` | Submissions per visitor and form, per minute. |
