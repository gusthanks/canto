# \PlaylistsApi

All URIs are relative to *https://api.spotify.com/v1*

Method | HTTP request | Description
------------- | ------------- | -------------
[**add_items_to_playlist**](PlaylistsApi.md#add_items_to_playlist) | **POST** /playlists/{playlist_id}/items | Add Items to Playlist 
[**add_tracks_to_playlist**](PlaylistsApi.md#add_tracks_to_playlist) | **POST** /playlists/{playlist_id}/tracks | Add Items to Playlist [DEPRECATED] 
[**change_playlist_details**](PlaylistsApi.md#change_playlist_details) | **PUT** /playlists/{playlist_id} | Change Playlist Details 
[**check_if_user_follows_playlist**](PlaylistsApi.md#check_if_user_follows_playlist) | **GET** /playlists/{playlist_id}/followers/contains | Check if Current User Follows Playlist 
[**create_playlist**](PlaylistsApi.md#create_playlist) | **POST** /me/playlists | Create Playlist 
[**create_playlist_for_user**](PlaylistsApi.md#create_playlist_for_user) | **POST** /users/{user_id}/playlists | Create Playlist for user 
[**follow_playlist**](PlaylistsApi.md#follow_playlist) | **PUT** /playlists/{playlist_id}/followers | Follow Playlist 
[**get_a_categories_playlists**](PlaylistsApi.md#get_a_categories_playlists) | **GET** /browse/categories/{category_id}/playlists | Get Category's Playlists 
[**get_a_list_of_current_users_playlists**](PlaylistsApi.md#get_a_list_of_current_users_playlists) | **GET** /me/playlists | Get Current User's Playlists 
[**get_featured_playlists**](PlaylistsApi.md#get_featured_playlists) | **GET** /browse/featured-playlists | Get Featured Playlists 
[**get_list_users_playlists**](PlaylistsApi.md#get_list_users_playlists) | **GET** /users/{user_id}/playlists | Get User's Playlists 
[**get_playlist**](PlaylistsApi.md#get_playlist) | **GET** /playlists/{playlist_id} | Get Playlist 
[**get_playlist_cover**](PlaylistsApi.md#get_playlist_cover) | **GET** /playlists/{playlist_id}/images | Get Playlist Cover Image 
[**get_playlists_items**](PlaylistsApi.md#get_playlists_items) | **GET** /playlists/{playlist_id}/items | Get Playlist Items 
[**get_playlists_tracks**](PlaylistsApi.md#get_playlists_tracks) | **GET** /playlists/{playlist_id}/tracks | Get Playlist Items [DEPRECATED] 
[**remove_items_playlist**](PlaylistsApi.md#remove_items_playlist) | **DELETE** /playlists/{playlist_id}/items | Remove Playlist Items 
[**remove_tracks_playlist**](PlaylistsApi.md#remove_tracks_playlist) | **DELETE** /playlists/{playlist_id}/tracks | Remove Playlist Items [DEPRECATED] 
[**reorder_or_replace_playlists_items**](PlaylistsApi.md#reorder_or_replace_playlists_items) | **PUT** /playlists/{playlist_id}/items | Update Playlist Items 
[**reorder_or_replace_playlists_tracks**](PlaylistsApi.md#reorder_or_replace_playlists_tracks) | **PUT** /playlists/{playlist_id}/tracks | Update Playlist Items [DEPRECATED] 
[**unfollow_playlist**](PlaylistsApi.md#unfollow_playlist) | **DELETE** /playlists/{playlist_id}/followers | Unfollow Playlist 
[**upload_custom_playlist_cover**](PlaylistsApi.md#upload_custom_playlist_cover) | **PUT** /playlists/{playlist_id}/images | Add Custom Playlist Cover Image 



## add_items_to_playlist

> models::ReorderOrReplacePlaylistsTracks200Response add_items_to_playlist(playlist_id, position, uris, add_tracks_to_playlist_request)
Add Items to Playlist 

Add one or more items to a user's playlist. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**position** | Option<**i32**> |  |  |
**uris** | Option<**String**> |  |  |
**add_tracks_to_playlist_request** | Option<[**AddTracksToPlaylistRequest**](AddTracksToPlaylistRequest.md)> |  |  |

### Return type

[**models::ReorderOrReplacePlaylistsTracks200Response**](reorder_or_replace_playlists_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## add_tracks_to_playlist

> models::ReorderOrReplacePlaylistsTracks200Response add_tracks_to_playlist(playlist_id, position, uris, add_tracks_to_playlist_request)
Add Items to Playlist [DEPRECATED] 

**Deprecated:** Use [Add Items to Playlist](/documentation/web-api/reference/add-items-to-playlist) instead.  Add one or more items to a user's playlist. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**position** | Option<**i32**> |  |  |
**uris** | Option<**String**> |  |  |
**add_tracks_to_playlist_request** | Option<[**AddTracksToPlaylistRequest**](AddTracksToPlaylistRequest.md)> |  |  |

### Return type

[**models::ReorderOrReplacePlaylistsTracks200Response**](reorder_or_replace_playlists_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## change_playlist_details

> change_playlist_details(playlist_id, change_playlist_details_request)
Change Playlist Details 

Change a playlist's name and public/private state. (The user must, of course, own the playlist.) 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**change_playlist_details_request** | Option<[**ChangePlaylistDetailsRequest**](ChangePlaylistDetailsRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## check_if_user_follows_playlist

> Vec<bool> check_if_user_follows_playlist(playlist_id, ids)
Check if Current User Follows Playlist 

Check to see if the current user is following a specified playlist.  **Note:** This endpoint is deprecated. Use [Check User's Saved Items](/documentation/web-api/reference/check-library-contains) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**ids** | Option<**String**> |  |  |

### Return type

**Vec<bool>**

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## create_playlist

> models::PlaylistObject create_playlist(create_playlist_request)
Create Playlist 

Create a playlist for the current Spotify user. (The playlist will be empty until you [add tracks](/documentation/web-api/reference/add-tracks-to-playlist).) Each user is generally limited to a maximum of 11000 playlists. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**create_playlist_request** | Option<[**CreatePlaylistRequest**](CreatePlaylistRequest.md)> |  |  |

### Return type

[**models::PlaylistObject**](PlaylistObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## create_playlist_for_user

> models::PlaylistObject create_playlist_for_user(user_id, create_playlist_request)
Create Playlist for user 

**Deprecated**: Use [Create Playlist](/documentation/web-api/reference/create-playlist) instead.  Create a playlist for a Spotify user. (The playlist will be empty until you [add tracks](/documentation/web-api/reference/add-tracks-to-playlist).) Each user is generally limited to a maximum of 11000 playlists. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**user_id** | **String** |  | [required] |
**create_playlist_request** | Option<[**CreatePlaylistRequest**](CreatePlaylistRequest.md)> |  |  |

### Return type

[**models::PlaylistObject**](PlaylistObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## follow_playlist

> follow_playlist(playlist_id, follow_playlist_request)
Follow Playlist 

Add the current user as a follower of a playlist.  **Note:** This endpoint is deprecated. Use [Save Items to Library](/documentation/web-api/reference/save-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**follow_playlist_request** | Option<[**FollowPlaylistRequest**](FollowPlaylistRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_a_categories_playlists

> models::PagingFeaturedPlaylistObject get_a_categories_playlists(category_id, limit, offset)
Get Category's Playlists 

Get a list of Spotify playlists tagged with a particular category. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**category_id** | **String** |  | [required] |
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingFeaturedPlaylistObject**](PagingFeaturedPlaylistObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_a_list_of_current_users_playlists

> models::PagingPlaylistObject get_a_list_of_current_users_playlists(limit, offset)
Get Current User's Playlists 

Get a list of the playlists owned or followed by the current Spotify user. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingPlaylistObject**](PagingPlaylistObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_featured_playlists

> models::PagingFeaturedPlaylistObject get_featured_playlists(locale, limit, offset)
Get Featured Playlists 

Get a list of Spotify featured playlists (shown, for example, on a Spotify player's 'Browse' tab). 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**locale** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingFeaturedPlaylistObject**](PagingFeaturedPlaylistObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_list_users_playlists

> models::PagingPlaylistObject get_list_users_playlists(user_id, limit, offset)
Get User's Playlists 

Get a list of the playlists owned or followed by a Spotify user. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**user_id** | **String** |  | [required] |
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingPlaylistObject**](PagingPlaylistObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_playlist

> models::PlaylistObject get_playlist(playlist_id, market, fields, additional_types)
Get Playlist 

Get a playlist owned by a Spotify user. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**market** | Option<**String**> |  |  |
**fields** | Option<**String**> |  |  |
**additional_types** | Option<**String**> |  |  |

### Return type

[**models::PlaylistObject**](PlaylistObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_playlist_cover

> Vec<models::ImageObject> get_playlist_cover(playlist_id)
Get Playlist Cover Image 

Get the current image associated with a specific playlist. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |

### Return type

[**Vec<models::ImageObject>**](ImageObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_playlists_items

> models::PagingPlaylistTrackObject get_playlists_items(playlist_id, market, fields, limit, offset, additional_types)
Get Playlist Items 

Get full details of the items of a playlist owned by a Spotify user.  **Note**: This endpoint is only accessible for playlists owned by the current user or playlists the user is a collaborator of. A `403 Forbidden` status code will be returned if the user is neither the owner nor a collaborator of the playlist. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**market** | Option<**String**> |  |  |
**fields** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]
**additional_types** | Option<**String**> |  |  |

### Return type

[**models::PagingPlaylistTrackObject**](PagingPlaylistTrackObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_playlists_tracks

> models::PagingPlaylistTrackObject get_playlists_tracks(playlist_id, market, fields, limit, offset, additional_types)
Get Playlist Items [DEPRECATED] 

**Deprecated:** Use [Get Playlist Items](/documentation/web-api/reference/get-playlists-items) instead.  Get full details of the items of a playlist owned by a Spotify user. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**market** | Option<**String**> |  |  |
**fields** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]
**additional_types** | Option<**String**> |  |  |

### Return type

[**models::PagingPlaylistTrackObject**](PagingPlaylistTrackObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## remove_items_playlist

> models::ReorderOrReplacePlaylistsTracks200Response remove_items_playlist(playlist_id, remove_items_playlist_request)
Remove Playlist Items 

Remove one or more items from a user's playlist. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**remove_items_playlist_request** | Option<[**RemoveItemsPlaylistRequest**](RemoveItemsPlaylistRequest.md)> |  |  |

### Return type

[**models::ReorderOrReplacePlaylistsTracks200Response**](reorder_or_replace_playlists_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## remove_tracks_playlist

> models::ReorderOrReplacePlaylistsTracks200Response remove_tracks_playlist(playlist_id, remove_tracks_playlist_request)
Remove Playlist Items [DEPRECATED] 

**Deprecated:** Use [Remove Playlist Items](/documentation/web-api/reference/remove-items-playlist) instead.  Remove one or more items from a user's playlist. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**remove_tracks_playlist_request** | Option<[**RemoveTracksPlaylistRequest**](RemoveTracksPlaylistRequest.md)> |  |  |

### Return type

[**models::ReorderOrReplacePlaylistsTracks200Response**](reorder_or_replace_playlists_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## reorder_or_replace_playlists_items

> models::ReorderOrReplacePlaylistsTracks200Response reorder_or_replace_playlists_items(playlist_id, uris, reorder_or_replace_playlists_tracks_request)
Update Playlist Items 

Either reorder or replace items in a playlist depending on the request's parameters. To reorder items, include `range_start`, `insert_before`, `range_length` and `snapshot_id` in the request's body. To replace items, include `uris` as either a query parameter or in the request's body. Replacing items in a playlist will overwrite its existing items. This operation can be used for replacing or clearing items in a playlist. <br/> **Note**: Replace and reorder are mutually exclusive operations which share the same endpoint, but have different parameters. These operations can't be applied together in a single request. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**uris** | Option<**String**> |  |  |
**reorder_or_replace_playlists_tracks_request** | Option<[**ReorderOrReplacePlaylistsTracksRequest**](ReorderOrReplacePlaylistsTracksRequest.md)> |  |  |

### Return type

[**models::ReorderOrReplacePlaylistsTracks200Response**](reorder_or_replace_playlists_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## reorder_or_replace_playlists_tracks

> models::ReorderOrReplacePlaylistsTracks200Response reorder_or_replace_playlists_tracks(playlist_id, uris, reorder_or_replace_playlists_tracks_request)
Update Playlist Items [DEPRECATED] 

**Deprecated:** Use [Update Playlist Items](/documentation/web-api/reference/reorder-or-replace-playlists-items) instead.  Either reorder or replace items in a playlist depending on the request's parameters. To reorder items, include `range_start`, `insert_before`, `range_length` and `snapshot_id` in the request's body. To replace items, include `uris` as either a query parameter or in the request's body. Replacing items in a playlist will overwrite its existing items. This operation can be used for replacing or clearing items in a playlist. <br/> **Note**: Replace and reorder are mutually exclusive operations which share the same endpoint, but have different parameters. These operations can't be applied together in a single request. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**uris** | Option<**String**> |  |  |
**reorder_or_replace_playlists_tracks_request** | Option<[**ReorderOrReplacePlaylistsTracksRequest**](ReorderOrReplacePlaylistsTracksRequest.md)> |  |  |

### Return type

[**models::ReorderOrReplacePlaylistsTracks200Response**](reorder_or_replace_playlists_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## unfollow_playlist

> unfollow_playlist(playlist_id)
Unfollow Playlist 

Remove the current user as a follower of a playlist.  **Note:** This endpoint is deprecated. Use [Remove Items from Library](/documentation/web-api/reference/remove-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## upload_custom_playlist_cover

> upload_custom_playlist_cover(playlist_id, content_type, body)
Add Custom Playlist Cover Image 

Replace the image used to represent a specific playlist. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**content_type** | **String** |  | [required] |
**body** | **String** |  | [required] |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: image/jpeg
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

