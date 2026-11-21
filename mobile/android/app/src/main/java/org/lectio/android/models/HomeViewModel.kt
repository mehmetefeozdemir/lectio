package org.lectio.android.models

import androidx.lifecycle.ViewModel
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import org.lectio.android.support.errorText
import org.lectio.android.support.title
import uniffi.lectio_ffi.CollectionInfo
import uniffi.lectio_ffi.CourseInfo
import uniffi.lectio_ffi.HomeFilter
import uniffi.lectio_ffi.LectioLibrary
import uniffi.lectio_ffi.SearchHitInfo
import uniffi.lectio_ffi.SessionInfo
import uniffi.lectio_ffi.SessionSort

/**
 * The home screen: courses, collections and lectures, filtered and sorted
 * by the core (the desktop's rules), and search across everything.
 */
class HomeViewModel(
    private val library: LectioLibrary,
    private val changed: () -> Unit = {},
) : ViewModel() {
    data class State(
        val filter: HomeFilter = HomeFilter.All,
        val sort: SessionSort = SessionSort.NEWEST,
        val query: String = "",
        val courses: List<CourseInfo> = emptyList(),
        val collections: List<CollectionInfo> = emptyList(),
        val sessions: List<SessionInfo> = emptyList(),
        val hits: List<SearchHitInfo> = emptyList(),
        val problem: String? = null,
    ) {
        val isSearching: Boolean get() = query.isNotBlank()

        /** The heading for the current filter. */
        val filterTitle: String
            get() = when (val f = filter) {
                is HomeFilter.All -> "All lectures"
                is HomeFilter.Course -> courses.firstOrNull { it.id == f.id }?.name ?: "Course"
                is HomeFilter.Collection -> collections.firstOrNull { it.id == f.id }?.name ?: "Collection"
                is HomeFilter.Label -> f.label.title
            }

        fun collectionsContaining(session: String): List<CollectionInfo> = collections.filter { session in it.sessionIds }
    }

    private val _state = MutableStateFlow(State())
    val state: StateFlow<State> = _state.asStateFlow()

    fun setFilter(filter: HomeFilter) {
        _state.update { it.copy(filter = filter) }
        reload()
    }

    fun setSort(sort: SessionSort) {
        _state.update { it.copy(sort = sort) }
        reload()
    }

    fun setQuery(query: String) {
        _state.update { it.copy(query = query) }
        search()
    }

    fun reload() {
        try {
            val courses = library.courses()
            val collections = library.collections()
            val current = _state.value
            val filter = if (filterExists(current.filter, courses, collections)) current.filter else HomeFilter.All
            val sessions = library.sessions(filter, current.sort)
            _state.update { it.copy(filter = filter, courses = courses, collections = collections, sessions = sessions) }
        } catch (e: Exception) {
            _state.update { it.copy(problem = errorText(e)) }
        }
        search()
    }

    private fun search() {
        val query = _state.value.query
        if (query.isBlank()) {
            _state.update { it.copy(hits = emptyList()) }
            return
        }
        try {
            val hits = library.search(query)
            _state.update { it.copy(hits = hits) }
        } catch (e: Exception) {
            _state.update { it.copy(problem = errorText(e)) }
        }
    }

    /** Returns the new course's id. */
    fun addCourse(name: String): String? = perform { library.addCourse(name.trim()) }

    /** Returns the new lecture's id. */
    fun addSession(courseId: String, title: String): String? = perform { library.addSession(courseId, title.trim()) }

    fun addCollection(name: String): String? = perform { library.addCollection(name.trim()) }

    fun deleteCollection(id: String) {
        perform { library.deleteCollection(id) }
    }

    fun addToCollection(session: String, collection: String) {
        perform { library.addToCollection(collection, session) }
    }

    fun removeFromCollection(session: String, collection: String) {
        perform { library.removeFromCollection(collection, session) }
    }

    /** Moves a lecture to the trash (the desktop can restore it). */
    fun trash(session: String) {
        perform { library.trashSession(session) }
    }

    fun clearProblem() = _state.update { it.copy(problem = null) }

    private fun filterExists(filter: HomeFilter, courses: List<CourseInfo>, collections: List<CollectionInfo>): Boolean = when (filter) {
        is HomeFilter.All, is HomeFilter.Label -> true
        is HomeFilter.Course -> courses.any { it.id == filter.id }
        is HomeFilter.Collection -> collections.any { it.id == filter.id }
    }

    private fun <T> perform(body: () -> T): T? = try {
        body().also {
            reload()
            changed()
        }
    } catch (e: Exception) {
        _state.update { it.copy(problem = errorText(e)) }
        null
    }

    companion object {
        /** A title for a new lecture: "Lecture, 20 Nov". */
        fun suggestedTitle(date: LocalDate = LocalDate.now()): String =
            "Lecture, " + date.format(DateTimeFormatter.ofPattern("d MMM", Locale.getDefault()))
    }
}
