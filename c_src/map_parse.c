/* map_parse.c — ALL FUNCTIONS MOVED TO c_src/sl_compat.c
 *
 * This file is intentionally empty of function bodies.
 * All active functions have been moved to sl_compat.c as part of the
 * C→Rust migration (task 15).
 *
 * File-scope globals (groups[][], val[], flags[], crctable[]) are now in sl_compat.c.
 *
 * The main packet dispatcher (clif_parse) was ported to Rust:
 *   src/game/map_parse/mod.rs
 *
 * All other clif_* and helper functions moved to sl_compat.c:
 *   getclifslotfromequiptype — moved to c_src/sl_compat.c
 *   replace_str — moved to c_src/sl_compat.c
 *   clif_getName — moved to c_src/sl_compat.c
 *   clif_Hacker — moved to c_src/sl_compat.c
 *   clif_sendurl — moved to c_src/sl_compat.c
 *   clif_sendprofile — moved to c_src/sl_compat.c
 *   clif_sendboard — moved to c_src/sl_compat.c
 *   CheckProximity — moved to c_src/sl_compat.c
 *   clif_accept2 — moved to c_src/sl_compat.c
 *   clif_timeout — moved to c_src/sl_compat.c
 *   clif_popup — moved to c_src/sl_compat.c
 *   clif_paperpopup — moved to c_src/sl_compat.c
 *   clif_paperpopupwrite — moved to c_src/sl_compat.c
 *   clif_paperpopupwrite_save — moved to c_src/sl_compat.c
 *   stringTruncate — moved to c_src/sl_compat.c
 *   clif_transfer — moved to c_src/sl_compat.c
 *   clif_transfer_test — moved to c_src/sl_compat.c
 *   clif_sendBoardQuestionaire — moved to c_src/sl_compat.c
 *   addtokillreg — moved to c_src/sl_compat.c
 *   clif_addtokillreg — moved to c_src/sl_compat.c
 *   clif_sendheartbeat — moved to c_src/sl_compat.c
 *   pc_sendpong — moved to c_src/sl_compat.c
 *   clif_getequiptype — moved to c_src/sl_compat.c
 *   nexCRCC — moved to c_src/sl_compat.c
 *   clif_debug — moved to c_src/sl_compat.c
 *   clif_user_list — moved to c_src/sl_compat.c
 *   clif_delay — moved to c_src/sl_compat.c
 *   clif_quit — moved to c_src/sl_compat.c
 *   clif_getlvlxp — moved to c_src/sl_compat.c
 *   clif_show_ghost — moved to c_src/sl_compat.c
 *   clif_getitemarea — moved to c_src/sl_compat.c
 *   clif_sendweather — moved to c_src/sl_compat.c
 *   checkevent_claim — moved to c_src/sl_compat.c
 *   dateevent_block — moved to c_src/sl_compat.c
 *   filler_block — moved to c_src/sl_compat.c
 *   gettotalscores — moved to c_src/sl_compat.c
 *   getevents — moved to c_src/sl_compat.c
 *   getevent_name — moved to c_src/sl_compat.c
 *   getevent_playerscores — moved to c_src/sl_compat.c
 *   clif_parseranking — moved to c_src/sl_compat.c
 *   clif_sendmob_side — moved to c_src/sl_compat.c
 *   clif_runfloor_sub — moved to c_src/sl_compat.c
 *   clif_parsedropitem — moved to c_src/sl_compat.c
 *   clif_mapmsgnum — moved to c_src/sl_compat.c
 *   clif_destroyold — moved to c_src/sl_compat.c
 *   clif_refreshnoclick — moved to c_src/sl_compat.c
 *   clif_sendupdatestatus — moved to c_src/sl_compat.c
 *   clif_sendupdatestatus2 — moved to c_src/sl_compat.c
 *   clif_getLevelTNL — moved to c_src/sl_compat.c
 *   clif_getXPBarPercent — moved to c_src/sl_compat.c
 *   clif_sendupdatestatus_onkill — moved to c_src/sl_compat.c
 *   clif_sendupdatestatus_onequip — moved to c_src/sl_compat.c
 *   clif_sendupdatestatus_onunequip — moved to c_src/sl_compat.c
 *   clif_parselookat_sub — moved to c_src/sl_compat.c
 *   clif_parselookat_scriptsub — moved to c_src/sl_compat.c
 *   clif_parselookat_2 — moved to c_src/sl_compat.c
 *   clif_parselookat — moved to c_src/sl_compat.c
 *   clif_parsechangepos — moved to c_src/sl_compat.c
 *   clif_parseviewchange — moved to c_src/sl_compat.c
 *   clif_parsefriends — moved to c_src/sl_compat.c
 *   clif_changeprofile — moved to c_src/sl_compat.c
 *   check_packet_size — moved to c_src/sl_compat.c
 *   canusepowerboards — moved to c_src/sl_compat.c
 *   clif_stoptimers — moved to c_src/sl_compat.c
 *   clif_handle_disconnect — moved to c_src/sl_compat.c
 *   clif_handle_missingobject — moved to c_src/sl_compat.c
 *   clif_handle_menuinput — moved to c_src/sl_compat.c
 *   clif_handle_powerboards — moved to c_src/sl_compat.c
 *   clif_handle_boards — moved to c_src/sl_compat.c
 *   clif_print_disconnect — moved to c_src/sl_compat.c
 *   metacrc — moved to c_src/sl_compat.c
 *   send_metafile — moved to c_src/sl_compat.c
 *   send_meta — moved to c_src/sl_compat.c
 *   send_metalist — moved to c_src/sl_compat.c
 *   clif_handle_obstruction — moved to c_src/sl_compat.c
 *   clif_sendtest — moved to c_src/sl_compat.c
 *   clif_parsemenu — moved to c_src/sl_compat.c
 *   clif_updatestate — moved to c_src/sl_compat.c
 *   clif_showboards — moved to c_src/sl_compat.c
 *   clif_isregistered — moved to c_src/sl_compat.c
 *   clif_getaccountemail — moved to c_src/sl_compat.c
 *   clif_clickonplayer — moved to c_src/sl_compat.c
 *   clif_object_canmove — moved to c_src/sl_compat.c
 *   clif_object_canmove_from — moved to c_src/sl_compat.c
 *   clif_changestatus — moved to c_src/sl_compat.c
 *   clif_postitem — moved to c_src/sl_compat.c
 *   clif_pushback — moved to c_src/sl_compat.c
 *   clif_cancelafk — moved to c_src/sl_compat.c
 *   clif_send — moved to c_src/sl_compat.c
 *   clif_sendtogm — moved to c_src/sl_compat.c
 *   clif_send_sub — moved to c_src/sl_compat.c
 *   clif_npc_move — moved to c_src/sl_compat.c
 *   clif_mob_move — moved to c_src/sl_compat.c
 *
 * Previously ported to Rust submodules:
 *   clif_parse — src/game/map_parse/mod.rs
 *   clif_sendack, clif_retrieveprofile, clif_screensaver, clif_sendtime,
 *   clif_sendid, clif_sendmapinfo, clif_sendxy, clif_sendxynoclick,
 *   clif_sendxychange, clif_sendstatus, clif_sendoptions, clif_mystaytus,
 *   clif_getchararea, clif_refresh, clif_sendminimap — src/game/map_parse/player_state.rs
 *   clif_blockmovement, clif_sendchararea, clif_charspecific, clif_parsewalk,
 *   clif_noparsewalk, clif_parsewalkpong, clif_parsemap, clif_sendmapdata,
 *   clif_sendside, clif_parseside — src/game/map_parse/movement.rs
 *   clif_lookgone, clif_cnpclook_sub, clif_cmoblook_sub, clif_charlook_sub,
 *   clif_object_look_sub, clif_object_look_sub2, clif_object_look_specific,
 *   clif_mob_look_start, clif_mob_look_close, clif_spawn — src/game/map_parse/visual.rs
 *   (and many others in combat.rs, chat.rs, dialogs.rs, items.rs, trading.rs,
 *    groups.rs, events.rs)
 */
