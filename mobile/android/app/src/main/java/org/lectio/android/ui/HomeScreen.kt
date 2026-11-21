package org.lectio.android.ui

import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Notes
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.FilterList
import androidx.compose.material.icons.filled.GraphicEq
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.foundation.background
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import org.lectio.android.models.HomeViewModel
import org.lectio.android.support.TimeText
import org.lectio.android.support.allLabels
import org.lectio.android.support.title
import uniffi.lectio_ffi.HomeFilter
import uniffi.lectio_ffi.SessionInfo
import uniffi.lectio_ffi.SessionSort

/** Courses, collections and lectures; search; new course, collection and lecture. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(home: HomeViewModel, selected: String?, onOpen: (String) -> Unit) {
    val state by home.state.collectAsStateWithLifecycle()
    var askingCourse by remember { mutableStateOf(false) }
    var askingCollection by remember { mutableStateOf(false) }
    var creatingLecture by remember { mutableStateOf(false) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(state.filterTitle, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.semantics { heading() }) },
                actions = { FilterMenu(home, state, onNewCourse = { askingCourse = true }, onNewCollection = { askingCollection = true }) },
            )
        },
        floatingActionButton = {
            if (state.courses.isNotEmpty()) {
                ExtendedFloatingActionButton(
                    text = { Text("New lecture") },
                    icon = { Icon(Icons.Filled.Add, contentDescription = null) },
                    onClick = { creatingLecture = true },
                    modifier = Modifier.testTag("home.newLecture"),
                )
            }
        },
    ) { padding ->
        Column(Modifier.padding(padding).fillMaxSize()) {
            OutlinedTextField(
                value = state.query,
                onValueChange = home::setQuery,
                placeholder = { Text("Search notes, slides and transcripts") },
                leadingIcon = { Icon(Icons.Filled.Search, contentDescription = null) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp).testTag("home.search"),
            )
            when {
                state.courses.isEmpty() -> StartWithACourse { askingCourse = true }
                state.isSearching -> SearchResults(state, onOpen)
                state.sessions.isEmpty() -> Hint("No lectures here yet. Tap New lecture to start one.")
                else -> LazyColumn(Modifier.fillMaxSize()) {
                    items(state.sessions, key = { it.id }) { session ->
                        SessionRow(session, session.id == selected, home, state, onOpen)
                    }
                }
            }
        }
    }

    if (askingCourse) {
        NameDialog("New course", "Course name", "home.courseName", { home.addCourse(it) }) { askingCourse = false }
    }
    if (askingCollection) {
        NameDialog("New collection", "Collection name", "home.collectionName", { home.addCollection(it) }) { askingCollection = false }
    }
    if (creatingLecture) {
        NewLectureDialog(state, onCreate = { course, title -> home.addSession(course, title)?.also(onOpen) }) { creatingLecture = false }
    }
    ProblemDialog(state.problem, home::clearProblem)
}

@Composable
private fun StartWithACourse(onNewCourse: () -> Unit) {
    Column(Modifier.fillMaxSize().padding(24.dp), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
        Text("Start with a course", style = MaterialTheme.typography.titleLarge, modifier = Modifier.semantics { heading() })
        Text("Lectures belong to a course, like \"Biology 101\".", color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(vertical = 8.dp))
        Button(onClick = onNewCourse, modifier = Modifier.testTag("home.newCourse")) { Text("New course") }
    }
}

@Composable
private fun Hint(text: String) {
    Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
        Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun SearchResults(state: HomeViewModel.State, onOpen: (String) -> Unit) {
    if (state.hits.isEmpty()) {
        Hint("Nothing matches \"${state.query}\".")
        return
    }
    LazyColumn(Modifier.fillMaxSize()) {
        items(state.hits) { hit ->
            Row(
                Modifier.fillMaxWidth()
                    .combinedClickable(onClick = { onOpen(hit.sessionId) })
                    .padding(horizontal = 16.dp, vertical = 12.dp)
                    .semantics(mergeDescendants = true) {
                        contentDescription = listOfNotNull(hit.sessionTitle, hit.snippet, hit.offsetMs?.let { "at " + TimeText.spoken(it) }).joinToString(", ")
                    },
            ) {
                Column(Modifier.weight(1f)) {
                    Text(hit.sessionTitle, style = MaterialTheme.typography.titleMedium)
                    Text(hit.snippet, maxLines = 3, overflow = TextOverflow.Ellipsis, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                hit.offsetMs?.let { Text(TimeText.clock(it), style = MaterialTheme.typography.labelMedium) }
            }
            HorizontalDivider()
        }
    }
}

@Composable
private fun SessionRow(session: SessionInfo, selected: Boolean, home: HomeViewModel, state: HomeViewModel.State, onOpen: (String) -> Unit) {
    var menu by remember { mutableStateOf(false) }
    val date = DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM).format(Instant.ofEpochMilli(session.createdUnixMs.toLong()).atZone(ZoneId.systemDefault()))
    val duration = session.durationMs
    val spoken = listOfNotNull(
        session.title, session.courseName, date,
        duration?.let { "recorded ${TimeText.spoken(it)}" },
        if (session.noteCount == 1u) "1 note" else "${session.noteCount} notes",
    ).joinToString(", ")
    Box {
        Column(
            Modifier.fillMaxWidth()
                .background(if (selected) MaterialTheme.colorScheme.secondaryContainer else Color.Transparent)
                .combinedClickable(onClick = { onOpen(session.id) }, onLongClick = { menu = true }, onLongClickLabel = "More actions")
                .padding(horizontal = 16.dp, vertical = 12.dp)
                .semantics(mergeDescendants = true) { contentDescription = spoken }
                .testTag("home.session"),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            Text(session.title, style = MaterialTheme.typography.titleMedium)
            Text("${session.courseName} · $date", color = MaterialTheme.colorScheme.onSurfaceVariant)
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                if (duration != null) Detail(Icons.Filled.GraphicEq, TimeText.clock(duration))
                Detail(Icons.AutoMirrored.Filled.Notes, session.noteCount.toString())
            }
        }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            state.collections.forEach { collection ->
                val inside = session.id in collection.sessionIds
                DropdownMenuItem(
                    text = { Text(if (inside) "Remove from ${collection.name}" else "Add to ${collection.name}") },
                    onClick = {
                        if (inside) home.removeFromCollection(session.id, collection.id) else home.addToCollection(session.id, collection.id)
                        menu = false
                    },
                )
            }
            DropdownMenuItem(text = { Text("Move to Trash") }, onClick = { home.trash(session.id); menu = false })
        }
    }
    HorizontalDivider()
}

@Composable
private fun Detail(icon: androidx.compose.ui.graphics.vector.ImageVector, text: String) {
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
        Icon(icon, contentDescription = null, modifier = Modifier.size(16.dp))
        Text(text, style = MaterialTheme.typography.bodyMedium)
    }
}

@Composable
private fun FilterMenu(home: HomeViewModel, state: HomeViewModel.State, onNewCourse: () -> Unit, onNewCollection: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    IconButton(onClick = { open = true }, modifier = Modifier.testTag("home.filter")) {
        Icon(Icons.Filled.FilterList, contentDescription = "Show and sort")
    }
    DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
        fun choose(filter: HomeFilter) {
            home.setFilter(filter)
            open = false
        }
        @Composable
        fun option(text: String, chosen: Boolean, onClick: () -> Unit) = DropdownMenuItem(
            text = { Text(text) },
            onClick = onClick,
            leadingIcon = { if (chosen) Icon(Icons.Filled.Check, contentDescription = "Chosen") },
        )
        option("All lectures", state.filter is HomeFilter.All) { choose(HomeFilter.All) }
        state.courses.forEach { option(it.name, state.filter == HomeFilter.Course(it.id)) { choose(HomeFilter.Course(it.id)) } }
        state.collections.forEach { option("Collection: ${it.name}", state.filter == HomeFilter.Collection(it.id)) { choose(HomeFilter.Collection(it.id)) } }
        allLabels.forEach { option("Label: ${it.title}", state.filter == HomeFilter.Label(it)) { choose(HomeFilter.Label(it)) } }
        HorizontalDivider()
        listOf(SessionSort.NEWEST to "Newest first", SessionSort.OLDEST to "Oldest first", SessionSort.TITLE to "Title", SessionSort.LONGEST to "Longest")
            .forEach { (sort, text) -> option("Sort: $text", state.sort == sort) { home.setSort(sort); open = false } }
        HorizontalDivider()
        option("New course", false) { open = false; onNewCourse() }
        option("New collection", false) { open = false; onNewCollection() }
        val filter = state.filter
        if (filter is HomeFilter.Collection) option("Delete this collection", false) { home.deleteCollection(filter.id); open = false }
    }
}

/** Asks for one name; [create] returns null when the core refused it (the screen shows why). */
@Composable
private fun NameDialog(title: String, label: String, tag: String, create: (String) -> String?, dismiss: () -> Unit) {
    var name by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = dismiss,
        title = { Text(title) },
        text = {
            OutlinedTextField(name, { name = it }, label = { Text(label) }, singleLine = true, modifier = Modifier.testTag(tag))
        },
        confirmButton = {
            TextButton(onClick = { create(name); dismiss() }, enabled = name.isNotBlank(), modifier = Modifier.testTag("dialog.create")) { Text("Create") }
        },
        dismissButton = { TextButton(onClick = dismiss) { Text("Cancel") } },
    )
}

/** Course (radio buttons) and title for a new lecture. */
@Composable
private fun NewLectureDialog(state: HomeViewModel.State, onCreate: (String, String) -> String?, dismiss: () -> Unit) {
    val preferred = (state.filter as? HomeFilter.Course)?.id ?: state.courses.firstOrNull()?.id.orEmpty()
    var course by remember { mutableStateOf(preferred) }
    var title by remember { mutableStateOf(HomeViewModel.suggestedTitle()) }
    AlertDialog(
        onDismissRequest = dismiss,
        title = { Text("New lecture") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                OutlinedTextField(title, { title = it }, label = { Text("Title") }, singleLine = true, modifier = Modifier.fillMaxWidth().testTag("newLecture.title"))
                Text("Course", style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(top = 12.dp))
                state.courses.forEach { option ->
                    Row(
                        Modifier.fillMaxWidth().selectable(selected = course == option.id, onClick = { course = option.id }, role = Role.RadioButton).padding(vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        RadioButton(selected = course == option.id, onClick = null)
                        Text(option.name, modifier = Modifier.padding(start = 8.dp))
                    }
                }
            }
        },
        confirmButton = {
            TextButton(
                onClick = { if (onCreate(course, title) != null) dismiss() },
                enabled = course.isNotEmpty() && title.isNotBlank(),
                modifier = Modifier.testTag("newLecture.create"),
            ) { Text("Create") }
        },
        dismissButton = { TextButton(onClick = dismiss) { Text("Cancel") } },
    )
}
