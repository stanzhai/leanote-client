var CommandPalette = {
    $mask: null,
    $input: null,
    $quickActions: null,
    $recents: null,
    $searchResults: null,
    $recentList: null,
    $searchResultList: null,
    $allItems: null,

    selectedIndex: -1,
    searchTimer: null,
    searchSeq: 0,
    wasInEditor: false,

    init: function() {
        var me = this;
        me.$mask = $('#commandPalette');
        me.$input = $('#cpSearchInput');
        me.$quickActions = $('#cpQuickActions');
        me.$recents = $('#cpRecents');
        me.$searchResults = $('#cpSearchResults');
        me.$recentList = $('#cpRecentList');
        me.$searchResultList = $('#cpSearchResultList');

        // Apply i18n to hardcoded HTML — lang.init() runs before this DOM exists
        me.$input.attr('placeholder', getMsg('searchNotes'));
        me.$mask.find('.lang').each(function () {
            var $el = $(this);
            var key = $.trim($el.text());
            if (key && getMsg(key) !== key) {
                $el.text(getMsg(key));
            }
        });

        // Global shortcut via capture phase — fires before the editor but
        // only calls preventDefault() (NOT stopPropagation), so the event
        // still reaches the editor's handlers and Vim state stays intact.
        document.addEventListener('keydown', function(e) {
            var keyCode = e.keyCode;
            if ((keyCode == 75 && !e.shiftKey) || (keyCode == 80 && e.shiftKey)) {
                if ((isMac() && e.metaKey) || (!isMac() && e.ctrlKey)) {
                    e.preventDefault();
                    if (me.$mask.is(':visible')) {
                        me.hide();
                    } else {
                        me.show();
                    }
                }
            }
        }, true);

        // Close on background click
        me.$mask.on('click', function(e) {
            if (e.target === me.$mask[0]) {
                me.hide();
            }
        });

        // Search input
        me.$input.on('input', function() {
            var val = $.trim(me.$input.val());
            if (val) {
                me.doSearch(val);
            } else {
                me.showRecentsMode();
            }
        });

        // Keyboard navigation within the palette
        me.$mask.on('keydown', function(e) {
            var keyCode = e.keyCode;

            // Prevent the global keydown handler from interfering
            if (keyCode == 27) { // Escape
                e.preventDefault();
                e.stopPropagation();
                me.hide();
                return;
            }

            if (keyCode == 38) { // Up
                e.preventDefault();
                e.stopPropagation();
                me.navigate(-1);
                return;
            }

            if (keyCode == 40) { // Down
                e.preventDefault();
                e.stopPropagation();
                me.navigate(1);
                return;
            }

            if (keyCode == 13) { // Enter
                e.preventDefault();
                e.stopPropagation();
                me.selectCurrent();
                return;
            }
        });

        // Prevent global keydown from firing when palette is open
        me.$input.on('keydown', function(e) {
            // Let navigation keys propagate to the mask handler
            if (e.keyCode === 27 || e.keyCode === 38 || e.keyCode === 40 || e.keyCode === 13) {
                return;
            }
            e.stopPropagation();
        });

        // Click handler for items (mouse click or tap)
        me.$mask.on('click', '.cp-item:not(.cp-no-results):visible', function(e) {
            e.preventDefault();
            e.stopPropagation();
            me.$allItems = me.$mask.find('.cp-item:not(.cp-no-results):visible');
            me.selectedIndex = me.$allItems.index(this);
            me.selectCurrent();
        });
    },

    show: function() {
        var me = this;
        // Detect if the active element is inside the editor — used to
        // restore focus via the editor API (not raw DOM focus) on hide.
        var active = document.activeElement;
        me.wasInEditor = active && (
            $(active).closest('#editorContent, #mdEditor, #wmd-input, .ace_editor').length > 0
        );
        me.$mask.show();
        me.$input.val('');
        me.selectedIndex = -1;
        me.searchSeq++;
        me.showRecentsMode();
        setTimeout(function() {
            me.$input.focus();
        }, 50);
    },

    hide: function() {
        var me = this;
        me.$mask.hide();
        me.$input.val('');
        me.selectedIndex = -1;
        if (me.searchTimer) {
            clearTimeout(me.searchTimer);
            me.searchTimer = null;
        }
        // Restore focus to the editor using its public API, not raw DOM
        // focus on the saved element. Ace's Vim keyboard handler needs a
        // proper focus cycle through editor.focus() to stay consistent.
        if (me.wasInEditor) {
            me.wasInEditor = false;
            setTimeout(function() {
                var note = Note.getCurNote();
                if (note && note.IsMarkdown && MD) {
                    MD.focus();
                } else if (tinymce && tinymce.activeEditor) {
                    tinymce.activeEditor.focus();
                }
            }, 0);
        }
    },

    showRecentsMode: function() {
        var me = this;
        me.$quickActions.show();
        me.$recents.show();
        me.$searchResults.hide();
        me.$searchResultList.empty();
        me.renderRecents();
        me.selectedIndex = -1;
        me.updateSelection();
    },

    showSearchMode: function() {
        var me = this;
        me.$quickActions.hide();
        me.$recents.hide();
        me.$searchResults.show();
        me.selectedIndex = -1;
    },

    doSearch: function(keyword) {
        var me = this;
        if (me.searchTimer) {
            clearTimeout(me.searchTimer);
        }
        me.searchTimer = setTimeout(function() {
            me.searchSeq++;
            var seq = me.searchSeq;
            NoteService.searchNote(keyword, function(notes) {
                if (seq !== me.searchSeq) return;
                me.showSearchMode();
                me.renderSearchResults(notes || []);
            });
        }, 300);
    },

    renderSearchResults: function(notes) {
        var me = this;
        var $list = me.$searchResultList;
        $list.empty();

        if (!notes || notes.length === 0) {
            $list.append('<div class="cp-item cp-no-results">' + getMsg('noResults') + '</div>');
            return;
        }

        for (var i = 0; i < notes.length; i++) {
            var note = notes[i];
            var $item = me.createNoteItem(note);
            $list.append($item);
        }
    },

    renderRecents: function() {
        var me = this;
        var $list = me.$recentList;
        $list.empty();

        // Collect all non-trashed, non-deleted notes and sort by UpdatedTime desc
        var recentNotes = [];
        for (var id in Note.cache) {
            if (!id) continue;
            var note = Note.cache[id];
            if (!note || note.IsTrash || note.IsDeleted || note.LocalIsDelete) continue;
            if (!('IsMarkdown' in note)) continue;
            recentNotes.push(note);
        }

        recentNotes.sort(function(a, b) {
            var t1 = a.UpdatedTime ? new Date(a.UpdatedTime).getTime() : 0;
            var t2 = b.UpdatedTime ? new Date(b.UpdatedTime).getTime() : 0;
            return t2 - t1;
        });

        var top6 = recentNotes.slice(0, 6);

        for (var i = 0; i < top6.length; i++) {
            var note = top6[i];
            var $item = me.createNoteItem(note);
            $list.append($item);
        }

        if (top6.length === 0) {
            $list.append('<div class="cp-item cp-no-results">' + getMsg('noResults') + '</div>');
        }
    },

    createNoteItem: function(note) {
        var title = (note.Title || getMsg('UnTitled')).replace(/</g, '&lt;').replace(/>/g, '&gt;');
        var notebookTitle = Notebook.getNotebookTitle ? Notebook.getNotebookTitle(note.NotebookId) : '';
        notebookTitle = (notebookTitle || '').replace(/</g, '&lt;').replace(/>/g, '&gt;');
        var html = '<div class="cp-item cp-note" data-noteid="' + note.NoteId + '">' +
            '<i class="fa fa-file-text-o"></i>' +
            '<span class="cp-note-title">' + title + '</span>' +
            '<span class="cp-note-notebook">' + notebookTitle + '</span>' +
            '</div>';
        return html;
    },

    navigate: function(direction) {
        var me = this;
        me.$allItems = me.$mask.find('.cp-item:not(.cp-no-results):visible');
        var len = me.$allItems.length;
        if (len === 0) return;

        me.selectedIndex += direction;
        if (me.selectedIndex < 0) me.selectedIndex = len - 1;
        if (me.selectedIndex >= len) me.selectedIndex = 0;

        me.updateSelection();
    },

    updateSelection: function() {
        var me = this;
        me.$allItems = me.$mask.find('.cp-item:not(.cp-no-results):visible');
        me.$allItems.removeClass('active');

        if (me.selectedIndex >= 0 && me.selectedIndex < me.$allItems.length) {
            var $selected = me.$allItems.eq(me.selectedIndex);
            $selected.addClass('active');

            // Scroll into view
            var container = $selected.closest('.cp-results');
            var scrollTop = container.scrollTop();
            var itemTop = $selected.position().top;
            var containerHeight = container.height();
            var itemHeight = $selected.outerHeight();

            if (itemTop + itemHeight > containerHeight) {
                container.scrollTop(scrollTop + itemTop + itemHeight - containerHeight + 10);
            } else if (itemTop < 0) {
                container.scrollTop(scrollTop + itemTop - 10);
            }
        }
    },

    selectCurrent: function() {
        var me = this;
        me.$allItems = me.$mask.find('.cp-item:not(.cp-no-results):visible');

        if (me.selectedIndex < 0 || me.selectedIndex >= me.$allItems.length) {
            // If nothing selected, pick the first item
            if (me.$allItems.length > 0) {
                me.selectedIndex = 0;
            } else {
                return;
            }
        }

        var $item = me.$allItems.eq(me.selectedIndex);
        if (!$item.length) return;

        // Quick action: new note
        if ($item.hasClass('cp-action')) {
            var action = $item.data('action');
            if (action === 'newNote') {
                me.hide();
                var notebookId = $('#curNotebookForNewNote').attr('notebookId');
                Note.newNote(notebookId);
            }
            return;
        }

        // Note item
        if ($item.hasClass('cp-note')) {
            var noteId = $item.data('noteid');
            if (noteId) {
                me.hide();
                Note.changeNoteForPjax(noteId, true, true);
                // Focus editor and enable writable mode after content loads
                setTimeout(function() {
                    var note = Note.getNote(noteId);
                    if (note && note.IsMarkdown && MD) {
                        MD.focus();
                    } else if (tinymce && tinymce.activeEditor) {
                        tinymce.activeEditor.focus();
                    }
                    Note.toggleWriteable(true);
                }, 300);
            }
        }
    }
};

$(function() {
    CommandPalette.init();
});
