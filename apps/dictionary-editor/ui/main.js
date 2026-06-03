// PSKK Dictionary Editor - Main JavaScript

console.log('PSKK Dictionary Editor loaded');

// Mock data for demonstration (will be replaced with Tauri commands later)
let mockEntries = [
    { reading: 'たなか', kanji: '田中', count: 5 },
    { reading: 'たなか', kanji: '棚下', count: 1 },
    { reading: 'ひろし', kanji: '博', count: 3 },
    { reading: 'ひろし', kanji: '宏', count: 2 },
    { reading: 'さとう', kanji: '佐藤', count: 4 },
];

let allEntries = [...mockEntries];
let filteredEntries = [...allEntries];
let selectedRows = new Set();
let sortColumn = null;
let sortDirection = 'asc';

// DOM Elements
const readingInput = document.getElementById('reading-input');
const kanjiInput = document.getElementById('kanji-input');
const addButton = document.getElementById('add-button');
const searchInput = document.getElementById('search-input');
const clearSearchButton = document.getElementById('clear-search-button');
const entriesTbody = document.getElementById('entries-tbody');
const entryCount = document.getElementById('entry-count');
const deleteButton = document.getElementById('delete-button');
const refreshButton = document.getElementById('refresh-button');
const closeButton = document.getElementById('close-button');

// Initialize
function init() {
    setupEventListeners();
    renderTable();
    updateEntryCount();
}

// Setup Event Listeners
function setupEventListeners() {
    // Add entry
    addButton.addEventListener('click', handleAdd);
    readingInput.addEventListener('keypress', (e) => {
        if (e.key === 'Enter') handleAdd();
    });
    kanjiInput.addEventListener('keypress', (e) => {
        if (e.key === 'Enter') handleAdd();
    });

    // Search
    searchInput.addEventListener('input', handleSearch);
    clearSearchButton.addEventListener('click', () => {
        searchInput.value = '';
        handleSearch();
    });

    // Actions
    deleteButton.addEventListener('click', handleDelete);
    refreshButton.addEventListener('click', handleRefresh);
    closeButton.addEventListener('click', handleClose);

    // Table sorting
    document.querySelectorAll('th.sortable').forEach(th => {
        th.addEventListener('click', () => handleSort(th.dataset.column));
    });

    // ESC key to close
    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape') handleClose();
    });
}

// Handle Add
function handleAdd() {
    const reading = readingInput.value.trim();
    const kanji = kanjiInput.value.trim();

    if (!reading) {
        alert('Please enter a reading (kana).');
        readingInput.focus();
        return;
    }

    if (!kanji) {
        alert('Please enter a kanji candidate.');
        kanjiInput.focus();
        return;
    }

    // Check if entry exists
    const existing = allEntries.find(e => e.reading === reading && e.kanji === kanji);
    if (existing) {
        existing.count++;
    } else {
        allEntries.push({ reading, kanji, count: 1 });
    }

    // Set search to show the added entry
    searchInput.value = reading;
    handleSearch();

    // Clear inputs
    readingInput.value = '';
    kanjiInput.value = '';
    readingInput.focus();

    renderTable();
    updateEntryCount();
}

// Handle Search
function handleSearch() {
    const query = searchInput.value.toLowerCase().trim();
    
    if (!query) {
        filteredEntries = [...allEntries];
    } else {
        filteredEntries = allEntries.filter(entry => 
            entry.reading.toLowerCase().includes(query) ||
            entry.kanji.toLowerCase().includes(query)
        );
    }

    renderTable();
    updateEntryCount();
}

// Handle Sort
function handleSort(column) {
    if (sortColumn === column) {
        sortDirection = sortDirection === 'asc' ? 'desc' : 'asc';
    } else {
        sortColumn = column;
        sortDirection = 'asc';
    }

    filteredEntries.sort((a, b) => {
        let aVal = a[column];
        let bVal = b[column];

        if (typeof aVal === 'string') {
            aVal = aVal.toLowerCase();
            bVal = bVal.toLowerCase();
        }

        if (sortDirection === 'asc') {
            return aVal > bVal ? 1 : -1;
        } else {
            return aVal < bVal ? 1 : -1;
        }
    });

    // Update sort indicators
    document.querySelectorAll('th.sortable').forEach(th => {
        th.classList.remove('sort-asc', 'sort-desc');
        if (th.dataset.column === column) {
            th.classList.add(`sort-${sortDirection}`);
        }
    });

    renderTable();
}

// Handle Delete
function handleDelete() {
    if (selectedRows.size === 0) {
        alert('Please select entries to delete.');
        return;
    }

    const count = selectedRows.size;
    if (!confirm(`Delete ${count} selected entry(s)?`)) {
        return;
    }

    // Remove selected entries
    const toDelete = Array.from(selectedRows);
    allEntries = allEntries.filter((_, index) => !toDelete.includes(index));
    
    selectedRows.clear();
    handleSearch(); // Refilter
    renderTable();
    updateEntryCount();
}

// Handle Refresh
function handleRefresh() {
    // In real implementation, this would reload from file
    // For now, just re-render
    selectedRows.clear();
    renderTable();
    updateEntryCount();
}

// Handle Close
function handleClose() {
    // In real implementation, this would close the Tauri window
    console.log('Close button clicked');
    alert('Close functionality will be implemented with Tauri commands');
}

// Render Table
function renderTable() {
    entriesTbody.innerHTML = '';
    selectedRows.clear();

    filteredEntries.forEach((entry, index) => {
        const row = document.createElement('tr');
        row.dataset.index = index;

        // Reading cell
        const readingCell = document.createElement('td');
        readingCell.textContent = entry.reading;
        row.appendChild(readingCell);

        // Kanji cell
        const kanjiCell = document.createElement('td');
        kanjiCell.textContent = entry.kanji;
        row.appendChild(kanjiCell);

        // Count cell (editable)
        const countCell = document.createElement('td');
        countCell.textContent = entry.count;
        countCell.classList.add('editable');
        countCell.title = 'Click to edit';
        countCell.addEventListener('click', () => handleCountEdit(entry, countCell));
        row.appendChild(countCell);

        // Row selection
        row.addEventListener('click', (e) => {
            if (e.target === countCell) return; // Don't toggle selection when editing count
            
            if (e.ctrlKey || e.metaKey) {
                // Multi-select
                row.classList.toggle('selected');
                if (row.classList.contains('selected')) {
                    selectedRows.add(index);
                } else {
                    selectedRows.delete(index);
                }
            } else {
                // Single select
                document.querySelectorAll('tbody tr').forEach(r => r.classList.remove('selected'));
                selectedRows.clear();
                row.classList.add('selected');
                selectedRows.add(index);
            }
        });

        entriesTbody.appendChild(row);
    });
}

// Handle Count Edit
function handleCountEdit(entry, cell) {
    const currentValue = entry.count;

    cell.textContent = '';
    cell.style.padding = '2px';

    const wrapper = document.createElement('div');
    wrapper.style.cssText = 'display:flex; align-items:center; gap:2px;';

    const input = document.createElement('input');
    input.type = 'number';
    input.min = '1';
    input.value = currentValue;
    input.style.cssText = 'width:50px; padding:2px 4px; text-align:right;';

    const decBtn = document.createElement('button');
    decBtn.textContent = '−';
    decBtn.style.cssText = 'width:20px; padding:0; cursor:pointer; line-height:1.4;';

    const incBtn = document.createElement('button');
    incBtn.textContent = '+';
    incBtn.style.cssText = 'width:20px; padding:0; cursor:pointer; line-height:1.4;';

    wrapper.appendChild(input);
    wrapper.appendChild(decBtn);
    wrapper.appendChild(incBtn);
    cell.appendChild(wrapper);
    input.focus();
    input.select();

    const saveEdit = () => {
        const newValue = parseInt(input.value);
        if (isNaN(newValue) || newValue < 1) {
            alert('Count must be at least 1.');
            input.focus();
            return;
        }
        entry.count = newValue;
        cell.style.padding = '';
        cell.textContent = newValue;
    };

    // Use pointerdown + preventDefault to adjust value without stealing focus
    decBtn.addEventListener('pointerdown', (e) => {
        e.preventDefault(); // keeps focus on input
        const val = parseInt(input.value) || 1;
        if (val > 1) {
            input.value = val - 1;
            entry.count = val - 1;
        }
    });

    incBtn.addEventListener('pointerdown', (e) => {
        e.preventDefault(); // keeps focus on input
        const val = parseInt(input.value) || 1;
        input.value = val + 1;
        entry.count = val + 1;
    });

    input.addEventListener('blur', saveEdit);

    input.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') {
            saveEdit();
        } else if (e.key === 'Escape') {
            cell.style.padding = '';
            cell.textContent = currentValue;
        }
    });

    input.addEventListener('input', () => {
        const newValue = parseInt(input.value);
        if (!isNaN(newValue) && newValue >= 1) {
            entry.count = newValue;
        }
    });
}

// Update Entry Count
function updateEntryCount() {
    const total = allEntries.length;
    const visible = filteredEntries.length;

    if (visible === total) {
        entryCount.textContent = `${total} entries`;
    } else {
        entryCount.textContent = `Showing ${visible} of ${total} entries`;
    }
}

// Initialize on load
init();
