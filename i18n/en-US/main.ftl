# Stalemate UI strings, en-US (DATA-006, CD-13). Drafted by Claude for the author to review.
#
# Voice: plain English, short. Review lines use the CD-08 coach voice: a friend on your side,
# casual second person, contractions, no profanity, 25 words or fewer per sentence. Numbers
# come only from placeables, never from the copy.

## App and navigation
app-title = Stalemate
nav-home = Home
nav-scouting = Scouting
nav-live = Live
nav-reviews = Reviews
nav-builds = Builds
nav-settings = Settings

## Demo controls (shown only with --demo)
demo-label = Demo mode
demo-cycle-state = Next data state
demo-toggle-offline = Toggle offline
demo-no-states = Open Scouting, Live, Reviews or Builds to try the data states.
demo-state-loaded = This view: loaded
demo-state-loading = This view: loading
demo-state-empty = This view: empty
demo-state-stale = This view: stale
demo-state-error = This view: error

## Data states (doc 09)
state-loading = Loading
state-retry = Try again
state-stale = Out of date: updated { $age }.
state-error = Couldn't reach { $source }.
offline-banner = You're offline. Saved data is still shown.

## Ages and counts
age-now = just now
age-minutes = { $n } min ago
age-hours = { $n ->
    [one] 1 hour ago
   *[other] { $n } hours ago
}
age-days = { $n ->
    [one] 1 day ago
   *[other] { $n } days ago
}
matches-count = { $n ->
    [one] { $n } match
   *[other] { $n } matches
}
value-missing = -
result-win = Win as { $hero }
result-loss = Loss as { $hero }
match-meta = { $duration } long, { $ago }
match-meta-short = { $duration } long

## Home
home-title = Home
home-checks-title = Setup checks
home-open-setup = Show setup steps
home-last-match-title = Last match
home-no-match = No matches yet. Play one and it shows up here.
check-pass = Pass
check-fail = Fail
check-pending = Pending
check-steam = Steam found
check-condebug = Launch option -condebug working
check-account-id = Account ID
check-broadcast = Broadcast
check-deadlock-api = deadlock-api reachable
check-steam-fail = Stalemate can't find Steam. Check that it's installed and has been run once.
check-condebug-fail = No console log yet. Add -condebug to Deadlock's launch options, then restart the game.
check-account-id-fail = Add your Account ID in Settings.
check-broadcast-fail = Broadcast isn't on. Turn it on in Deadlock's settings.
check-deadlock-api-fail = Can't reach deadlock-api. Check your connection.

## First-run setup (-condebug)
setup-title = Set up Deadlock
setup-intro = Stalemate learns when a match starts from a log file Deadlock writes. Deadlock only writes it with the -condebug launch option.
setup-steps-title = Add the launch option
setup-step-1 = 1. In Steam, open your Library and right-click Deadlock.
setup-step-2 = 2. Choose Properties, then stay on the General tab.
setup-step-3 = 3. In Launch Options, type -condebug.
setup-step-4 = 4. Start Deadlock, reach the main menu, then press Check again.
setup-check-again = Check again
setup-close = Close
setup-result-pass = It's working
setup-result-fail = Not working yet
setup-result-pending = Not checked yet
setup-pass-hint = Stalemate can see Deadlock's log. You're set.
setup-fail-hint = No log yet. Check the option is spelled exactly -condebug, then restart Deadlock.

## Scouting (FP-10, doc 05)
scouting-title = Scouting
scouting-hidden = Scouting appears once the match starts.
scouting-empty = No scouting data for this match.
team-amber = Amber
team-sapphire = Sapphire
team-heading-yours = Your team: { $team }
team-heading-enemy = Enemy team: { $team }
card-you = You
card-loading = Loading this player
no-public-data = No public data
stat-overall = Overall
stat-last = Last { $n } matches
stat-last-few = Last { $n } matches (fewer than { $window })
stat-rank = Rank
stat-trend = MMR trend
stat-on-hero = On this hero
stat-mains = Top heroes
record-line = { $wins } W / { $losses } L ({ $pct }%)
record-none = No matches
rank-none = No rank yet
trend-unavailable = Not available yet
trend-flat = Flat
trend-up = Up { $n } per 10 matches
trend-down = Down { $n } per 10 matches
main-record = { $matches } matches, { $pct }%

## Live (doc 06)
live-title = Live
live-delay-label = Broadcast delay
live-delay-value = { $secs } seconds
live-delay-unknown = Not known yet
live-delay-stale = Stale: the delay is high
live-empty = No match in progress.
live-board-title = Enemy board
live-col-hero = Hero
live-col-player = Player
live-col-items = Items
live-col-kda = K/D/A
live-col-level = Level
items-none = No items yet
log-title = Purchase log
log-col-time = Time
log-col-player = Player
log-col-item = Item
log-col-action = Action
log-empty = No purchases yet.
log-buy = Buy
log-sell = Sell
log-upgrade = Upgrade

## Reviews (doc 07, stats only)
reviews-title = Reviews
reviews-empty = No matches to review yet.
reviews-pick = Pick a match to see its report.
report-duration = Match length { $duration }
report-summary-win = You took that one in { $minutes } minutes. Nice work.
report-summary-loss = That one got away from you after { $minutes } minutes. Shake it off, you've got a good one coming.
report-top-source = Most of your souls came from { $source }.
report-lane-title = Lane souls at { $time }
report-lane-value = { $souls } souls
report-lane-ahead = You were { $souls } souls up at { $time }. That's a strong lane, so keep leaning on it.
report-lane-even = You and your lane were dead level at { $time }. Look for one small edge next time.
report-lane-behind = You were { $souls } souls behind at { $time }. Tighter last hits will close that gap fast.
report-lane-unavailable = Not available
report-lane-unavailable-line = That match ended before { $time }, so there's no lane number. Next one, I'll have it for you.
report-souls-title = Where your souls came from
report-stats-only = This is a stats-only report for now. Deeper coaching comes in a later version.
soul-lane-troopers = Lane troopers
soul-neutral-camps = Neutral camps
soul-breakables = Breakables
soul-urn = Urn
soul-sinners-sacrifice = Sinner's Sacrifice
soul-amount = { $souls } souls ({ $pct }%)

## Builds (doc 08)
builds-title = Builds
builds-tab-library = Library
builds-tab-import = Community builds
builds-tab-builder = Build maker
builds-empty = No builds yet. Import one or make your own.
builds-active = Active
builds-set-active = Make active
build-meta = { $source }, { $count ->
    [one] 1 item
   *[other] { $count } items
}
build-source-ingame = In-game build
build-source-external = Imported
build-source-manual = Made by you
build-needs-update = Needs update: { $n ->
    [one] 1 item removed
   *[other] { $n } items removed
}
import-intro = Search the community builds Deadlock offers for a hero, then import the one you like.
import-hero = Hero
import-search = Search
import-button = Import
import-empty = No search yet. Pick a hero and press Search.
community-usage = Used in { $n } matches
builder-hero = Hero
builder-name-placeholder = Build name
builder-section-placeholder = Section name
builder-target = Add items here
builder-target-on = Adding here
builder-section-empty = No items in this section yet.
builder-move-up = Move up
builder-move-down = Move down
builder-remove = Remove
builder-add-section = Add section
builder-search-title = Find items
builder-search-placeholder = Search items by name
builder-add-item = Add to the selected section
builder-ability-title = Ability order (optional)
builder-ability-placeholder = For example: Ability 1, Ability 2, Ability 1
builder-save = Save build
builder-error-duplicate = That item is already in this build.
builder-error-no-section = Pick a section first.
builder-error-no-hero = Pick a hero first.
builder-error-no-name = Give the build a name.
builder-error-empty = Add at least one item.

## Settings and About (FP-9)
settings-title = Settings
settings-account-title = Account ID
settings-account-help = Your Account ID is the number in your Steam friend code. Steam shows it under Friends, then Add a Friend.
settings-account-placeholder = Account ID, numbers only
settings-account-save = Save
settings-account-invalid = That isn't a valid Account ID. Use numbers only.
settings-llm-title = AI coach
settings-llm-label = Provider
settings-llm-off = Off
settings-llm-note = The AI coach is off. Reviews use plain stats text.
settings-scale-title = Text size
settings-scale-help = Make everything in the window smaller or larger.
settings-privacy-title = Privacy
settings-privacy-1 = Stalemate asks deadlock-api for public match data. Players who opted out show as No public data.
settings-privacy-2 = Match recordings stay on your computer. They include chat, and Stalemate never shows it.
settings-privacy-3 = Stalemate never reads Deadlock's memory, changes its files or sends it input.
about-title = About
about-version = Version { $version }
about-licence = Free software under the GPL-3.0-or-later licence.
about-not-affiliated = Stalemate isn't affiliated with or endorsed by Valve.
about-fonts = Fonts: Jost and Noto Sans, under the SIL Open Font License.
