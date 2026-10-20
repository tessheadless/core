---
sidebar_position: 5
title: Calendars
---

# Calendars

Admins link calendars to Tess in **Admin → Calendars**, for reservations and
events handled by [n8n](n8n.md) workflows:

| Calendar | Role | In n8n |
|---|---|---|
| **Google Calendar** | Receives events | **Tess → Calendar**: create, update and cancel events; Google emails the invitations. List events. |
| **cal.diy** booking page | Sends bookings | **Tess Trigger** on `calendar.booking.created`, `…rescheduled`, `…cancelled`; **Tess → Calendar → Get Bookings**. |

Credentials stay in Tess, encrypted with `TESS_SECRETS_KEY`. Workflows only
use a Tess API token, so you can change accounts without touching them.

Typical flows:

- **Reservation form**: a [form](forms.md) `form.submitted` → **Create Event**
  in the "Reservations" Google calendar, with the visitor as attendee.
- **Booking page**: a guest books on cal.diy → `calendar.booking.created` →
  confirm on Slack, add the guest to your CRM, or mirror the booking into a
  Google calendar.
- **Event content**: an `Event` entry is published → **Create Event** with the
  date and place of the entry; `entry.unpublish` → **Cancel Event**.

## Google calendars

### 1. The Google OAuth client (once)

In the [Google Cloud console](https://console.cloud.google.com/):

1. Enable the **Google Calendar API** for your project.
2. Configure the **OAuth consent screen** (internal for a Google Workspace
   domain; external otherwise, adding your accounts as test users until the
   app is published).
3. Create an **OAuth client ID** of type **Web application**, with this
   **authorized redirect URI** (shown in **Admin → Calendars**):

   ```text
   https://cms.example.com/api/calendars/google/callback
   ```

4. Enter the client id and secret in **Admin → Calendars → Google OAuth
   client** (the secret is stored encrypted), or set
   `TESS_GOOGLE_CLIENT_ID` and `TESS_GOOGLE_CLIENT_SECRET` on the server.

Tess asks for the `calendar` scope (events and invitations) and the account's
email address.

### 2. Link a calendar

**Link a Google calendar** opens Google's sign-in. After consent, Google sends
you back to the admin. Pick the calendar to link: any calendar the account can
write to, including shared and group calendars. Link several calendars of the
same account with **Link a calendar** next to it.

Tess keeps the account's refresh token, encrypted, and refreshes access tokens
by itself. Removing the account in Tess doesn't revoke Tess's access at Google:
also remove it in the account's [Google security settings](https://myaccount.google.com/permissions).

### Events and invitations

From n8n, **Tess → Calendar**:

| Operation | Fields |
|---|---|
| Create Event | Calendar, title, start, end, attendees, description, location, time zone, **Send Invitations** |
| Update Event | Event ID, and the fields to change |
| Cancel Event | Event ID; **Send Invitations** tells the attendees |
| Get Events | From, to (default: the next 30 days) |

Dates are RFC 3339 date-times (`2026-12-24T19:00:00+01:00`), local date-times
with a time zone (`2026-12-24T19:00:00` and `Europe/Paris`, the calendar's zone
by default), or `YYYY-MM-DD` for all-day events. Attendees are emails,
optionally with a name: `Ada <ada@example.com>, grace@example.com`.

With **Send Invitations**, Google emails the invitation, the updates and the
cancellation to the attendees, from the linked account.

Each change emits `calendar.event.created`, `…updated` or `…cancelled`.

## cal.diy booking pages

[cal.diy](https://github.com/calcom/cal.diy) is the MIT-licensed,
self-hostable fork of Cal.com: visitors book a slot on its pages. Tess receives
each booking change as a webhook, signed with a secret, and turns it into an
event for your workflows.

1. In **Admin → Calendars → Link a cal.diy booking page**, give a name and the
   public URL of the booking page.
2. Tess shows a **webhook URL** and a **secret** (shown once).
3. In cal.diy, **Settings → Developer → Webhooks → New**: paste the URL and the
   secret, and choose the triggers (booking created, rescheduled, cancelled…).

Each booking then emits an event, with the booking normalized and the original
cal.diy payload in `raw`:

```json
{
  "event": "calendar.booking.created",
  "data": {
    "calendar": { "id": "…", "name": "Table booking", "provider": "CALDIY" },
    "booking": {
      "trigger": "BOOKING_CREATED", "uid": "…", "title": "Table for 2",
      "start": "2026-12-24T18:00:00Z", "end": "2026-12-24T20:00:00Z",
      "attendees": [{ "email": "ada@example.com", "name": "Ada", "timeZone": "Europe/Paris" }],
      "organizer": { "email": "chef@example.com", "name": "Chef" }
    },
    "raw": { "triggerEvent": "BOOKING_CREATED", "payload": { "…": "…" } }
  }
}
```

| cal.diy trigger | Tess event |
|---|---|
| `BOOKING_CREATED` | `calendar.booking.created` |
| `BOOKING_RESCHEDULED` | `calendar.booking.rescheduled` (`rescheduleUid` is the replaced booking) |
| `BOOKING_CANCELLED` | `calendar.booking.cancelled` |
| `BOOKING_REQUESTED`, `BOOKING_REJECTED` | `calendar.booking.requested`, `…rejected` |
| Any other | `calendar.booking.updated` |

Requests without a valid `x-cal-signature-256` get a `403`. **New secret**
replaces it, and the webhook in cal.diy must be updated. The last 100
bookings are listed in **Admin → Calendars → Bookings**.

Tess doesn't create cal.diy bookings: visitors book on cal.diy's pages.

### cal.diy with Docker Compose

The `caldiy` profile runs cal.diy next to Tess, **on the same PostgreSQL
server**, so you operate a single database server. cal.diy gets its own
`caldiy` database and role, created by the `caldiy-db-init` service, and Tess
uses the `tess` database:

```bash
scripts/init-env.sh --calendar caldiy     # Tess on PostgreSQL + cal.diy secrets
docker compose --profile caldiy up -d     # starts postgres, caldiy-db-init, caldiy and tess
```

| Setting | |
|---|---|
| `TESS_DATABASE=postgres`, `DATABASE_URL`, `POSTGRES_PASSWORD` | Tess on the shared server (`init-env.sh --calendar caldiy` sets them). |
| `CALDIY_PUBLIC_URL` | Where browsers reach cal.diy (default `http://localhost:3001`). |
| `CALDIY_DB_PASSWORD` | Password of cal.diy's `caldiy` database role. |
| `CALDIY_NEXTAUTH_SECRET`, `CALDIY_ENCRYPTION_KEY` | cal.diy's own secrets. |
| `CALDIY_SMTP_*`, `CALDIY_EMAIL_FROM` | Optional: booking confirmation emails. |
| `CALDIY_GOOGLE_API_CREDENTIALS` | Optional: Google Calendar inside cal.diy (availability). |

cal.diy publishes no Docker image yet, so Compose **builds it from source** on
first start. Pin the source with `CALDIY_REF`, a commit of the cal.diy
repository. The build takes several minutes and about 6 GB of memory; later
starts reuse the image. Open cal.diy at `CALDIY_PUBLIC_URL` and create its
admin account.

In the same Compose project, cal.diy reaches Tess at `http://tess:8080`: use
`http://tess:8080/api/calendars/<id>/webhook` as the subscriber URL.

cal.diy's own documentation recommends it for personal, non-production use.

## Permissions

| | Owner / Admin | Contributor | Reviewer, Member |
|---|:-:|:-:|:-:|
| Link and unlink calendars, Google client | ✔ | | |
| Create, update, cancel events | ✔ | ✔ with the `calendar` scope | |
| Read events and bookings | ✔ | ✔ with the `calendar` scope | ✔ with the `calendar` scope |

An empty scope gives access to everything, `calendar` included. For an n8n
workflow, an API token with the Contributor role and the scope `calendar` is
enough.

## GraphQL API

```graphql
query { calendars { id name provider enabled timeZone bookingUrl } }

mutation($c: ID!) {
  createCalendarEvent(calendarId: $c, sendInvitations: true, input: {
    title: "Dinner for 4", start: "2026-12-24T19:00:00", end: "2026-12-24T21:00:00",
    timeZone: "Europe/Paris", attendees: [{ email: "ada@example.com", name: "Ada" }]
  }) { id link attendees { email status } }
}

query($c: ID!) { calendarEvents(calendarId: $c, from: "2026-12-01T00:00:00Z", to: "2027-01-01T00:00:00Z") { id title start end } }
query($c: ID!) { calendarBookings(calendarId: $c) { event uid title start attendees { email } } }
```

## Settings

| Variable | Default | |
|---|---|---|
| `TESS_CALENDARS` | `true` | `false` turns calendars off. |
| `TESS_GOOGLE_CLIENT_ID`, `TESS_GOOGLE_CLIENT_SECRET` | unset | Google OAuth client; replaces the one set in the admin. |
