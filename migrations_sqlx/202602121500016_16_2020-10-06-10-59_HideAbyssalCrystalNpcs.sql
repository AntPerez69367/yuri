-- Move Abyssal crystals into an unused map to hide them without deleting
UPDATE NPCs0
SET NpcMapId = 1071, NpcX = 9, NpcY = 9
WHERE NpcId BETWEEN 364 AND 379;
