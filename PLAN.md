# PLAN.md — Arcade Economy & Multiplayer Roadmap

## Phase 1: Blackjack (First Multiplayer Game)

**Goal:** Drop-in PvE card game. Multiple players at the same table, each playing against the dealer independently. Zero coordination needed — one person can play alone, others can join/leave freely.

### Why Blackjack first

- No matchmaking needed (PvE, not PvP)
- Drop-in/drop-out: doesn't break if someone disconnects
- Simple rules, fast hands (~30 seconds each)
- Natural chip sink — betting is the whole point
- Social: players at the same table see each other's hands, can chat

### Game design

- **One global table** (or one per "room" later). Players sit down, bet chips, play against a dealer bot.
- **Standard Blackjack rules:** hit, stand, double down, split. Dealer stands on soft 17.
- **Betting:** min bet 10 chips, max bet 100 chips (adjustable). Bet from your chip balance. Winnings added, losses subtracted (floor enforced).
- **Turn flow:** all players bet → cards dealt → each player acts in turn → dealer reveals → payouts.
- **Async-safe:** if a player disconnects mid-hand, auto-stand on their current hand.
- **Activity feed:** "🃏 @mat won 50 chips at Blackjack" on big wins.

### Architecture considerations

- Blackjack state lives in-memory per table (like vote rounds). No need to persist mid-hand.
- Table state broadcast to all seated players via existing per-session channels.
- Chip balance changes are DB writes (through ChipService).
- The table can be a new screen subsection within The Arcade, or a dedicated game entry.

### What this does NOT include (yet)

- No player-vs-player betting
- No private tables
- No card counting countermeasures (it's a cozy clubhouse, not a casino)
- No chat-based `/play blackjack` command (join from Arcade lobby)

---

## Phase 2: Future (Not Planned Yet)

### Monthly chip leaderboard resets
- Archive monthly chip leaders (top 3 get a permanent badge?)
- Reset balances to baseline at month end
- "Hall of Fame" display somewhere

### Strategy multiplayer (Chess, Battleship)
- No chips needed — W/L record + rating
- Async: make a move, come back later
- Game completion counts toward daily streaks
- `/challenge @user chess` in chat for matchmaking

### More casino games (Poker)
- Texas Hold'em: PvP, uses chip betting
- Needs turn management, pot logic, hand evaluation
- Higher complexity — build after Blackjack validates the chip system

### Chat-based matchmaking
- Activity feed broadcast when someone sits at an empty table
- `/play <game>` and `/challenge @user <game>` commands
- Accept/decline prompts

---

## Game category model (unified view)

| Category | Games | Win condition | Leaderboard section | Streaks | Chips |
|----------|-------|--------------|-------------------|---------|-------|
| Daily puzzles | Sudoku, Nonograms, Minesweeper, Solitaire | Solve the daily | Today's Champions | Yes | +50 bonus per completion |
| High-score | Tetris, 2048 | Personal best | All-Time High Scores | No | No |
| Casino | Blackjack, Poker (future) | Grow your chip balance | Chip Leaders | Optional | Bet and win/lose |
| Strategy | Chess, Battleship (future) | Beat opponent | W/L + Rating | Yes (game completed) | No |
