const fs = require('fs');
const path = 'c:\Users\37653\Desktop\higher_copy.db'.replace(/\\\\/g, '/');

console.log('Attempting to read DB with simple file inspection...');
try {
  const buf = fs.readFileSync(path);
  console.log('DB size:', buf.length);
  console.log('Header string:', buf.toString('utf8', 0, 16));
  
  // Try to find evaluation records by searching for strings in the binary file
  const searchStrings = ['Evaluation UI 验收测试', 'Recall UI 验收'];
  for (const s of searchStrings) {
    const idx = buf.indexOf(Buffer.from(s, 'utf8'));
    if (idx >= 0) {
      console.log('Found record: "' + s + '" at byte offset', idx);
    } else {
      console.log('NOT found: "' + s + '"');
    }
  }
} catch(e) {
  console.log('Error:', e.message);
}
