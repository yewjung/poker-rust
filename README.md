```mermaid
---
title: Poker game state diagram
---
stateDiagram-v2
    state NOT_ENOUGH_PLAYERS
    state PRE_FLOP
    state FLOP
    state TURN
    state RIVER
    state SHOWDOWN

    state pre_flop_betting_done <<choice>>
    state flop_betting_done <<choice>>
    state turn_betting_done <<choice>>
    state river_betting_done <<choice>>
    state new_game <<choice>>
    state start_game <<choice>>
    
    [*] --> NOT_ENOUGH_PLAYERS
    note right of NOT_ENOUGH_PLAYERS: wait for at least 2 players to join
    NOT_ENOUGH_PLAYERS --> start_game
    start_game --> PRE_FLOP: if at least 2 players are ready
    start_game --> NOT_ENOUGH_PLAYERS: otherwise
    note right of PRE_FLOP: deal cards to players<br> placed small bind and big bind bets
    PRE_FLOP --> pre_flop_betting_done
    pre_flop_betting_done --> FLOP: if all players have bet the same amount
    pre_flop_betting_done --> PRE_FLOP: otherwise
    FLOP --> flop_betting_done
    flop_betting_done --> TURN: if all players have bet the same amount
    flop_betting_done --> FLOP: otherwise
    TURN --> turn_betting_done
    turn_betting_done --> RIVER: if all players have bet the same amount
    turn_betting_done --> TURN: otherwise
    RIVER --> river_betting_done
    river_betting_done --> SHOWDOWN: if all players have bet the same amount
    river_betting_done --> RIVER: otherwise
    note right of SHOWDOWN: determine the winner<br> distribute the pot<br>remove disconnected players, add new players
    SHOWDOWN --> new_game
    new_game --> PRE_FLOP: if at least 2 players are still in the game
    new_game --> NOT_ENOUGH_PLAYERS: otherwise
```

### Run server without Docker
```bash
DATABASE_URL=postgres://user:password@localhost:5432/my_database RUST_LOG=debug cargo run
```