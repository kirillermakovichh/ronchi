# Dashboard

Status: implemented local desktop interface. No stable API compatibility promise.
This document owns browser behavior, access, live transport and UI evidence limits.
The [shared interface contract](investigation-interfaces.md) owns query semantics and CLI/MCP operations.
The [evidence contract](evidence-and-findings.md) owns Journal facts and Findings.
Build tools and source ownership live in [development](../development.md).

## Workspace

Workspace is the default destination. Journal and Lab are separate destinations.
Cases sits above Observations on the left, Traffic in the middle, and Inspector on the right.
All four widgets stay mounted and visible.
Drag a full panel title onto another panel to exchange positions.
A drag overlay and highlighted target show the pending move.
Shared pointer/keyboard dividers resize adjacent columns or the stacked left panels.
Reset restores the default layout; add, remove, hide, collapse and maximize controls are absent.
Non-sensitive layout preferences are stored per project within the browser origin.
A different viewer port has separate preferences.
Small laptops through desktop monitors are the primary target; narrow widths receive basic responsive handling.

Observations groups committed Fault applications into episodes before a Finding or follow-up exists.
Findings linked to the same application share one row. Another Fault application creates another episode, even for the same signature.
Selecting an episode opens its timed path and highlights related Traffic. Only related explicitly filters the list.
Multiple path steps can stay expanded.
A Traffic request opens Overview, Request, Response and Records tabs. Back to Observation restores the path.
UI episodes create no Journal facts or core Findings. Record order does not establish application causality.

Request detail displays retained metadata and explains that the original request body was not retained.
Prepared content, HTTP transport result and unknown client receipt remain distinct.
Journal exposes all retained record kinds without requiring a Finding.
Lab shows current runtime/configuration and execution boundaries.
Shared filters cover signature, method, request ID, Case activation, kind and time/sequence interval.
The filter control shows the active-field count and supports edit/clear.
Exact request detail follows its ID across list filters.
Browser Back restores selection. Forms, detail tabs, payload previews and expanded steps survive relevant live updates.
Inactive destinations release subscriptions. Virtualized lists preserve their retained visible anchor within the bounded window.

The header has separate mini-cards for `RPS avg`, `RPS active` and Lab health.
Health describes Ronchi, not the backend or a transaction. Journal finalization is not shown in this card.
Rate tooltips explain average-with-idle and active intervals briefly.
See [the visual direction](positioning-and-brand.md#visual-identity) for palette and typography.

## Access and retained viewing

For a ready Lab, run `ronchi dashboard --lab-id <immutable-id>`.
This finite command prints a launch URL and never opens a browser.
`--json` returns a version-1 envelope.
The fragment contains a reusable access key valid until this Lab stops.
Every launch command for the same live Lab returns the same key.
The browser removes that fragment before exchanging it for an HttpOnly, SameSite=Strict cookie.
The server accepts that cookie until the Lab stops; it has no 30-minute timeout.
The cookie has no Max-Age or Expires; browsers may discard it when their own session ends.
Reopen the same Lab link to restore browser access.
The Lab retains exactly one launch key and one distinct browser key after first launch.
Opening more tabs or browser contexts does not grow an authorization pool.
A new Lab gets new keys even if its name or port is reused.
Launch metadata reports `expires_in_seconds:null` and `expires_on:"lab_stop"`.
Cookies have Lab- and port-specific names and Lab-specific paths.
Browser session exchange and WebSocket upgrade require the actual Host and same Origin.
Bearer control requests retain their existing authorization rules.
Do not share launch URLs as evidence links.
Evidence bookmarks use the current credential-free page URL; there is no evidence-link copy control.

For retained evidence, run `ronchi dashboard --serve --lab-id <immutable-id>`.
This foreground process binds an OS-selected loopback port and prints one readiness record.
With `--json`, readiness is a version-1 envelope with `read_only:true`, a reusable launch URL, `expires_in_seconds:null`, and `expires_on:"viewer_stop"`.
The retained viewer uses the same fixed-size access model; its keys last until that viewer exits.
The process then waits until Ctrl-C or stdout pipe closure.
It cancels readers and drains its owned connections within three seconds before forcing closure.
It never starts a simulator, writer, source application, or relayer.
It does not modify the Lab manifest.
Its routes expose read-only Investigation queries.
It rejects live Labs; use the ordinary finite dashboard launch command for those.
The viewer remains bound to the resolved immutable Lab even if another Lab reuses its name.
Retained viewing has no live RPC endpoint.

`up` includes an optional top-level `dashboard` launch object.
The Demo includes `dashboard_launch` for a ready Lab and `dashboard_argv` for recovery or retained viewing.
A launch transport failure does not undo a successful Lab startup or Demo.
Run the exact-ID dashboard command for the Lab link if launch metadata is absent.
No command opens a browser automatically.
Closing the browser leaves the persistent Lab running.

## Browser WebSocket transport

After the initial cookie exchange, the browser uses one `/ui/{id}/socket` WebSocket for data and commands.
There are no browser HTTP data routes or periodic browser reads.
The existing `/v1/labs/{id}/...` bearer API remains available to agents.
Static assets and `/ui/{id}/session` remain HTTP infrastructure.
Operation selectors such as `api/investigate` inside frames do not issue HTTP requests.

The server sends a version-1 `ready` frame with the immutable Lab ID.
The browser sends correlated `call`, `subscribe`, `unsubscribe`, `cancel` and `visibility` frames.
The server sends `reply` or `update` frames with the same ID, a status and a typed core result or error.
Read subscriptions reuse finite core queries.
Writer commits and runtime publication mark affected subscriptions dirty.
The server coalesces changes and sends the newest bounded view.
This is a live view, not an event-delivery guarantee; complete retained history remains in the Journal.
A view without `snapshot_seq` selects the current committed watermark for each evaluation.
A view with an explicit watermark stays fixed.
Payload previews require explicit one-shot calls.

Each Lab/viewer admits at most eight sockets.
Each socket admits 16 subscriptions, two concurrent jobs and four queued calls.
Each Hub admits two concurrent browser queries to the shared core reader pool.
Waiting query jobs remain bounded by the socket/job limits.
Incoming frames are limited to 401 KiB.
Ordinary bodies retain the 64 KiB query cap; Custom Case validation/activation bodies allow 400 KiB.
Outgoing envelopes allow 320 KiB; core responses retain their 256 KiB cap.
Socket output buffers allow 512 KiB, with a two-second send deadline.
Slow clients disconnect without blocking Journal writes.
Each operation has a 30-second transport deadline in addition to core limits.
The runtime owns upgrade callbacks and cancels/joins their jobs on disconnect or shutdown.

A hidden page suspends new subscription refreshes; in-flight replies can still arrive.
Visibility restoration sends one signal and resynchronizes the subscribed views.
A disconnected browser reconnects with delays from one to ten seconds.
Only read subscriptions are restored.
Pending commands fail with unknown effects if they were sent.
The browser never replays a mutation automatically.
A rejected read retains prior data and is reevaluated on the next relevant change or explicit reselection.
The read-only retained viewer uses the same transport and rejects mutations.

## Bookmarks and payloads

The shared query core returns at most five Findings in summary; the paged Findings operation remains available to agents.
Workspace groups committed Fault applications into Observation episodes and links their Findings.
Opening a detail keeps live updates on. Pause or an exact snapshot URL fixes Investigation evidence.
Changing scope resets page navigation. A failed continuation keeps the last valid page visible.

Bookmark URLs retain the immutable Lab ID, snapshot, semantic scope, and selected event or Finding.
They omit the bootstrap ticket, credentials, and cursor.
Bootstrap removes only the ticket fragment.
Integer coordinates retain the full signed SQLite integer range without conversion through an imprecise JavaScript number.
Invalid or unavailable references display an error rather than selecting the latest snapshot.
To move a bookmark to a new live session or retained endpoint, pass its query to the launch command:

```bash
ronchi dashboard --lab-id <id> --selection 'snapshot=42&event=7'
ronchi dashboard --serve --lab-id <id> --selection 'snapshot=42&event=7'
```

The CLI rejects credential, cursor, unknown and oversized selection fields before requesting a ticket or binding a viewer.
A live endpoint does not survive Lab stop.
The interface shows exact-ID retained viewer instructions for its current selection.

Payload previews load only on explicit action and follow `payload.next_offset` in UTF-8 bytes.
Repeated clicks while busy do not queue work.
Automatic compact lookup-result reads have the separate bounds below.
Signature copy uses an original 14px CSS glyph in a 24px button.
A successful Clipboard write shows a check for one second.
Live updates of the same signature preserve that confirmation; unmount clears its timer.

## Live investigation and recent pages

Ordinary navigation and detail URLs without a snapshot remain live.
Live refresh preserves the selected record or Finding, detail tab, form values, payload preview and keyboard focus.
Selecting another record can move focus to its heading; background refresh does not.
A read failure retains prior data.
Relevant state changes or explicit reselection reevaluate that subscription.
After a connection loss, the browser restores read subscriptions without replaying commands.
Visibility restoration catches up with the current Journal watermark.
Runtime status remains current while an Investigation is paused.

Traffic and Journal show up to 100 newest matching records in descending registration/event order.
Older pages retain an explicit upper sequence boundary and still refresh related exchange facts at new watermarks.
`Latest traffic` returns to the moving latest page.
The transaction chain also uses bounded recent pages; it displays each page chronologically and offers earlier records separately.
No browser view accumulates the complete Journal.


## Case activity and history

Case controls show an acknowledged success message after arm, replace, custom activation and disarm.
Current Case activity displays the UTC activation time, activation revision and committed Fault application count.
The native writer publishes the counter only after commit, without a polling query or retained Journal scan.
A new activation resets it.
Late records from a previous activation do not increase the new count.
The count covers hold starts, changed lookup entries, assigned confirmation schedules and proven-expired Case blockhash returns.
A shared HTTP hold counts once per exchange, including a batch.
This is a count of committed effect records, not unique business incidents or affected transfers.
Missing or mismatched telemetry shows unavailable rather than zero.
Historical Journals without the new timestamp or telemetry retain explicit unavailable labels.

Empty Observations explains the absence of matching applied-fault records. Unsupported analysis is labelled separately from recorded fault applications.
An empty result never implies that no Fault occurred or that the backend passed a test.

RPC detail and Finding detail include a same-signature transaction chain across the list filters at the pinned snapshot.
The chain shows received requests, backend source records, acceptance, selected Faults, withholding, HTTP boundary records and later correlated calls when retained.
Each stage has its recorded time, source, relative elapsed time and exact Journal bookmark.
Stage presence follows recorded facts; no synthetic execution or client-receipt stages are added.
Inspector history reads contain at most 200 metadata records; technical continuation remains bounded.
Continuation remains explicit and cancels when the detail changes or closes.
An incomplete page cannot establish absence of later calls.
The end of retained records is limited to the pinned snapshot and the observed HTTP path.

## Human-readable Observations

Observation cards lead with a plain-language title and short explanation.
Opening an Observation shows a short outcome, inline request counts and a compact timed timeline.
The Inspector shows the selected-history bound and partial-evidence state. The global header and Traffic footer omit snapshot labels.
Caveats, supporting references, identifiers and retained replies appear under Technical details.
This presentation covers response withholding, later lookups/submissions, changed lookup views, simulator confirmation delay, pre-dispatch withholding, expired blockhash and the supported Custom lookup template.
Unknown rules retain a conservative explanation and access to their exact statement.

The UI groups existing evidence; it does not create new Journal facts or change Finding rules.
Each step links to its supporting source records at an explicit snapshot.
The source links remain separate from the readable sentence.
Counts use distinct `rpc.registered` request IDs, not backend chunks, prepared responses or other transport stages.
The first follow-up can show elapsed real time from the original request.
Simulator progression durations use explicitly labelled simulated time.
A configured withholding limit is labelled as a limit, not as an observed client timeout.

The story uses one bounded evidence subscription for the same signature or the request IDs linked by the Finding.
It examines at most 200 metadata records under the existing response cap.
It preserves the selected scope and fixed/live behavior.
If subscriptions arrive at different watermarks, the story uses their common retained prefix.
Truncation, restrictive event/method/request filters, interrupted Journals, legacy correlation and missing context expose partial evidence.
Partial counts are lower bounds.
Only a complete same-signature selection can support the statement that no repeat submission was recorded in that selection.
This does not establish absence on other RPC paths or outside the selected interval.

One Technical details disclosure retains the current technical story, identifiers, Finding statement, observation window, transaction history, raw evidence, pagination and payload previews.
The technical panels remain readable and update live while open.
Client timeouts, retry causes, business completion and economic execution require separate supporting evidence.

## Compact RPC result timeline

The timeline shows rounded real elapsed seconds from the original received request.
Missing time coordinates remain unknown.
Negative relative times identify retained RPC evidence before the original submission.
Intentional Ronchi Faults use a coral pause icon and a Ronchi fault label.
Execution errors use a red failure icon and an execution-error label.
Confirmed/finalized replies with an explicit null `err` use a green check.
Processed replies use an amber clock and say that confirmation is pending.
Neutral rows distinguish a null status, RPC error, pending response, unavailable reply and unconfirmed delivery.
Color never carries meaning alone.
Simulated confirmation is labelled as simulated RPC state.
A confirmed RPC status does not establish application business completion.

The result comes from the retained application-facing lookup reply, not transaction acceptance or an unrelated simulator transition.
Signature-array position and exact request/Lab IDs bind a status entry to the selected transaction.
The UI requires a recorded completed HTTP server write before presenting a returned reply.
This still does not prove client receipt.
Prepared metadata is used when it contains the result.
For a separate source exchange, the UI verifies identity, chunk schema/order/completeness and retention/redaction flags.
If Ronchi changed the view, the selected replacement must match the prepared lookup digest.
A known single replacement entry is labelled `verified_entry_only` in technical data; it is not presented as a reconstructed full batch response.
Unverifiable or absent results remain unknown.

The UI automatically inspects up to 16 recent received lookup requests.
Separate source responses are limited to 32 KiB and two retained chunks per lookup.
Two workers use existing finite WebSocket evidence calls with byte-offset continuation.
A 32-entry cache retains immutable results.
Workers are cancelled on selection replacement or hidden destinations.
Pushed resynchronization can retry failed reads without a polling timer.
Manual raw payload continuation remains available for larger retained responses.
The summary uses the latest observed status reply; a later getTransaction without an explicit confirmation level cannot invent a new confirmation status.
A later observed execution error remains visible.
Consecutive matching lookup outcomes can be grouped with a count and their observed time range.
Long timelines retain up to six lookup groups and four additional RPC actions in the compact view.
The existing technical history remains paginated.

## Lab RPC rates

The header shows `RPS avg` and `RPS active` beside Lab health.
Both metrics cover the entire Lab, independent of Traffic filters, pagination, Pause and selected snapshots.
A registered JSON-RPC batch member counts as one call.
Unsupported or invalid calls count if Ronchi records `rpc.registered` for them.
Unregistered transport failures do not count.
The metrics do not measure the independently started backend's lifetime, processing time or success rate.

- **RPS avg:** registered calls divided by real elapsed time from the first registration to the current time while the Lab runs, or to its terminal event after shutdown. Idle time is included.
- **RPS active:** registered calls divided by the sum of active arrival intervals. Each interval runs from its first registration to its last registration. A gap of at least ten seconds starts a new interval. The idle tail after the last registration is excluded immediately. Each interval has a one-second minimum duration, including a singleton or a simultaneous batch.

The average denominator also has a one-second minimum to avoid undefined startup values.
With no calls, both values show `0.0`.
Display uses one decimal place.
Tooltips explain the scope and interval rules.
If counters or the live connection are unavailable, values show `—` until fresh data arrives.
After an interrupted history without a terminal record, values freeze at the last recorded event and show `last recorded`.
That interval does not establish the actual shutdown time.

The existing status response exposes optional `rpc_traffic` counters and real-time references.
The Journal writer owns a constant-size aggregate and publishes changes only after commit.
A failed write does not increase the counters.
Concurrent registrations that arrive out of timestamp order use the previous registration time as a lower bound.
Live status snapshots add the current monotonic Lab time.
The browser extrapolates that reference with a local display clock during idle time.
The clock sends no HTTP requests or WebSocket frames.
Stopped counters never advance with browser time.
Retained readers replay the original records once when opening status/viewer, with constant application memory and no Journal mutation.
No storage migration, new agent operation or periodic history scan is needed.
