# Wine Diary — the screens Art and his wife click through (#4523)

Wren leads the screens, Kade makes them clickable, Silas's `wine-diary/docs/screen-fields.md`
maps every field to the model. Jeff 2026-10-13 (via Silas): Art and his wife can't see rough
work and then tell us what we missed. So every screen below is finished: real copy, real-looking
data, no placeholders, no question marks.

Each person keeps their **own diary**: they add, rate and share their own bottles, and see only
their own unless someone grants them Read (Jeff 16:29: no shared household; one role, member).
Two people in the demo: **Art**, and **{PARTNER}**, a separate user Art invited. Each rates their own
bottles: **Again · Fine · Not for us**. Art has shared his diary with her (Read), so she can see his
bottles and verdicts, never change them. A web page on any phone, on our own sign-in server.

Sample cellar used on every screen (consistent across screens):

Art's diary (what {PARTNER} sees too, read-only, because he shared it):

| Wine | Producer · region | Vintage | Bought | Art's verdict |
|---|---|---|---|---|
| Chianti Classico | Castello di Volpaia · Tuscany, Italy | 2020 | Social Wines, $27 · 14 Sep | Again |
| Sancerre | Domaine Vacheron · Loire, France | 2022 | Bauer Wine & Spirits, $38 · 2 Oct | Fine |
| Rioja Reserva | La Rioja Alta "Viña Ardanza" · Rioja, Spain | 2016 | Social Wines, $42 · 2 Oct | In the cellar |
| Malbec | Catena · Mendoza, Argentina | 2021 | Trader Joe's, $16 · 21 Sep | Not for us |
| Prosecco | Bisol "Crede" · Veneto, Italy | NV | Bauer Wine & Spirits, $22 · 2 Oct | Again |

{PARTNER}'s own diary: Sancerre 2022 (her own bottle, from the same shop) rated Again, and an Albariño, Pazo de Señoráns 2023, Rías Baixas, $24 · 5 Oct, rated Fine.

## 0 · Sign in

- Title: **The Wine Diary**. Line under it: "Your own wine diary."
- One button: **Sign in**. Below, small: "By invitation only."
- First time {PARTNER} opens Art's invite: "Art invited you to keep a wine diary, and shared his with you." Button: **Join**.
- Done state: lands on Your wines, greeting "Good evening, Art."

## 1 · Your wines

- Title: **Your wines** · count "5 bottles".
- Search box: "Search a wine, a place, a person…"
- One row per bottle: label photo thumbnail, wine and vintage, producer, where and when bought, and your verdict badge (**Again**). A bottle not opened yet says "In the cellar".
- Two tabs at the top: **Mine** and **Shared with me**. {PARTNER} sees "Art's diary" under Shared with me, read-only.
- Big round camera button, bottom right: **Add a bottle**.
- Tapping a row opens that bottle (the wine page: everything below in one place).

## 2 · Snap

- Full-screen camera view of a label. One line: "Point at the front label."
- Shutter button. After the shot: the photo, and "Reading the label…" for a moment.
- Small toggle under the shutter: "Remember where I bought it" (on).

## 3 · Check what it read

- Title: **Is this right?**
- The photo at the top. Then the fields it read, each editable in place:
  Producer **Castello di Volpaia**, Wine **Chianti Classico**, Vintage **2020**, Region **Tuscany, Italy**.
- Section **Bought**: Store **Social Wines** (picked from your stores), Price **$27**, Date **14 Sep**.
- Two buttons: **Save** (primary) and **Fix something**.
- Done state: "Saved. It's in your cellar." and back to Your wines with the new row on top.

## 4 · Rate when opened

- Title: the wine, e.g. **Chianti Classico 2020**. Line: "Opened Saturday, 11 Oct."
- **Who was there**: Art, {PARTNER}, and guests added by name ("the Dunleavys"). Guests are names, never accounts.
- **Your verdict**, three large buttons: **Again · Fine · Not for us**. You rate your own bottles. On a wine someone shared with you, you see their verdict ("Art said Again") but can't change it.
- **A note** (optional): "Lovely with the lamb."
- Done state: "Noted." The verdict badge shows on Your wines.

## 5 · Find it

- Same search box as Your wines, results as you type.
- Example query **"Italian, Social Wines"** → Chianti Classico 2020, Again, bought 14 Sep for $27, opened with the Dunleavys, "Lovely with the lamb."
- Searches label, grape, region, store, price, date, note and guests, across your diary and anything shared with you.

## 6 · Show a friend

- From a wine page: **Share this wine**. From Your wines: **Share the list**.
- Enter a name and email: "Send an invite". They get their own diary, and can look at what you shared, not change it.
- List of who can see what, each with **Remove**. Removing ends their access at once: "Removed. They can no longer see it."
- This is how Art brought {PARTNER} in: an invite, plus his whole diary shared with her (Read). She keeps her own diary and can share hers back.

## 7 · Settings

- **Weekly reminder**: on, Sunday morning. One line: "At most one gentle nudge a week."
- **Remember where I bought it**: on.
- **Shared with**: who can see your diary or a wine, each with Remove.
- **Sign out**.

## What changed from the 1 Oct flow

- A web page on any phone, not an iPhone app.
- Shared by invitation on our own sign-in server, not iCloud.
- Ratings are Again · Fine · Not for us, one per person, not 1–5 stars.
- Region and country sit on the wine, as the screens show them.

## Gaps the screens needed from the model: closed (Silas, wine-diary cf8d428, 16:28)

takenAtPlace (where bought), readBarcode, reminderDay, rememberWhereBought are in the model.
Region and country are on Wine too. Catalog match (LWIN / Open Food Facts) is deliverable 2,
so no screen shows it.
