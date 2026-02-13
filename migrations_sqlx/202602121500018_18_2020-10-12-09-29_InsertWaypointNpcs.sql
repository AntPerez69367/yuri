CREATE TEMPORARY TABLE tmptable_1 SELECT * FROM NPCs0 WHERE NpcId IN ('52', '209', '289');
UPDATE tmptable_1
SET NpcId = 392,
	NpcIdentifier = 'InnNpc',
	NpcDescription = 'Cithra',
	NpcMapId = 3812,
	NpcX = 19,
	NpcY = 4
WHERE NpcId = 52;
UPDATE tmptable_1
SET NpcId = 393,
	NpcIdentifier = 'NoxhilNpc',
	NpcDescription = 'Noxhil',
	NpcMapId = 1031,
	NpcX = 5,
	NpcY = 4
WHERE NpcId = 209;
UPDATE tmptable_1
SET NpcId = 394,
	NpcIdentifier = 'StelsiNpc',
	NpcDescription = 'Stelsi',
	NpcMapId = 522,
	NpcX = 6,
	NpcY = 2,
	NpcMoveTime = 8000,
	NpcReturnDistance = 3
WHERE NpcId = 289;
INSERT INTO NPCs0 SELECT * FROM tmptable_1;
DROP TEMPORARY TABLE IF EXISTS tmptable_1;
