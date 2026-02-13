CREATE TEMPORARY TABLE tmptable_1 SELECT * FROM Spells WHERE SplId = 3167;
UPDATE tmptable_1 SET SplId = 4729, SplIdentifier = 'magis_bane_poet', SplPthId = 4;
INSERT INTO Spells SELECT * FROM tmptable_1;
DROP TEMPORARY TABLE IF EXISTS tmptable_1;
UPDATE Spells SET SplDescription = 'Magi\'s Bane' WHERE SplDescription = 'Magi\'s bane';
