CREATE TEMPORARY TABLE tmptable_1 SELECT * FROM Mobs WHERE MobId = 562;
UPDATE tmptable_1 SET MobId = 807, MobIdentifier = 'cotw_giasomo_bird', MobDescription = 'Giasomo bird', MobLook = 58, MobLookColor = 0;
INSERT INTO Mobs SELECT * FROM tmptable_1;
DROP TEMPORARY TABLE IF EXISTS tmptable_1;
