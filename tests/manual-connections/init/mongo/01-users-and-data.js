const lab = db.getSiblingDB('bookie_lab');
lab.createUser({
  user: 'bookie_rw',
  pwd: 'bookie_rw_password',
  roles: [{ role: 'readWrite', db: 'bookie_lab' }],
});
lab.createUser({
  user: 'bookie_ro',
  pwd: 'bookie_ro_password',
  roles: [{ role: 'read', db: 'bookie_lab' }],
});
lab.people.insertMany([
  { _id: 1, name: 'Ada Lovelace', email: 'ada@example.test', active: true },
  { _id: 2, name: 'Grace Hopper', email: 'grace@example.test', active: false },
]);
