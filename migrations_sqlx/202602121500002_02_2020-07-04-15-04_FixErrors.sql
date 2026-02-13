-- ----------------------------
-- Having no records in the Character table can cause issues, so add two dummy characters.
-- ----------------------------
INSERT INTO `Character` (`ChaName`, `ChaPassword`) VALUES ('Reserved1', 'rsvd1');
INSERT INTO `Character` (`ChaName`, `ChaPassword`) VALUES ('Reserved2', 'rsvd2');
-- ----------------------------
-- Fix typo in Foraged Fields 1 map name
-- ----------------------------
UPDATE Maps SET MapName = 'Foraged Fields 1' WHERE MapName = ' Foraged Fields 1';
-- ----------------------------
-- Map 7200 does not exist, so move all NPCs from that map to a test map
-- ----------------------------
UPDATE NPCs0 SET NpcMapId = 1071, NpcX = 9, NpcY = 9 WHERE NpcMapId = 7200;
